use anyhow::Result;
use chrono::Utc;
use contracts::domain::a009_ozon_returns::aggregate::OzonReturns;
use contracts::domain::a010_ozon_fbs_posting::aggregate::OzonFbsPosting;
use contracts::domain::a011_ozon_fbo_posting::aggregate::OzonFboPosting;
use contracts::domain::a012_wb_sales::aggregate::WbSales;
use contracts::domain::a013_ym_order::aggregate::YmOrder;
use contracts::domain::a014_ozon_transactions::aggregate::OzonTransactions;
use contracts::domain::a016_ym_returns::aggregate::YmReturn;
use uuid::Uuid;

use super::repository::Model;
use crate::projections::p906_nomenclature_prices;
use crate::shared::marketplaces::wildberries::datetime::wb_business_date_str;

/// Константа эквайринга ПРОДАЖИ (1.9%)
const WB_ACQUIRING_RATE: f64 = 0.019;

/// Helper функция для получения себестоимости из p906_nomenclature_prices
/// Возвращает None если nomenclature_ref отсутствует или цена не найдена
async fn get_cost_for_nomenclature(
    nomenclature_ref: &Option<String>,
    sale_date: &str,
) -> Result<Option<f64>> {
    Ok(
        p906_nomenclature_prices::service::resolve_price_for_optional_nomenclature(
            nomenclature_ref.as_deref(),
            sale_date,
        )
        .await?
        .map(|resolved_price| resolved_price.price),
    )
}

fn normalize_cost_sign(cost: Option<f64>, is_return: bool) -> Option<f64> {
    cost.map(|value| {
        let abs_cost = value.abs();
        if is_return {
            abs_cost
        } else {
            -abs_cost
        }
    })
}

pub async fn from_wb_sales_lines(document: &WbSales, document_id: &str) -> Result<Vec<Model>> {
    let now = Utc::now().to_rfc3339();
    let id = Uuid::new_v4().to_string();

    // Получаем значения из строки документа
    let total_price = document.line.total_price.unwrap_or(0.0);
    let price_list = document.line.price_list.unwrap_or(0.0);
    let finished_price = document.line.finished_price.unwrap_or(0.0);
    let amount_line = document.line.amount_line.unwrap_or(0.0);
    let price_effective = document.line.price_effective.unwrap_or(0.0);
    let spp = document.line.spp.unwrap_or(0.0);

    // Расчёт сумм по формулам:
    // 1. total_price -> price_full
    let price_full = total_price;

    // 2. price_list -> price_list (без изменений)
    // let price_list = price_list;

    // 3. Если price_effective > 0, то customer_in = finished_price, иначе customer_out = finished_price
    let (customer_in, customer_out, price_return) = if price_effective > 0.0 {
        (finished_price, 0.0, 0.0)
    } else {
        (0.0, finished_price, price_list)
    };

    // 4. amount_line - price_effective -> commission_out
    let commission_out = amount_line - price_effective;

    // 5. spp -> coinvest_persent
    let coinvest_persent = spp;

    // 6. commission_out / price_effective * 100 -> commission_percent (округляем до 2 знаков)
    let commission_percent = if price_effective != 0.0 {
        ((commission_out / price_effective * 100.0) * 100.0).round() / 100.0
    } else {
        0.0
    };

    // 7. finished_price * ACQUIRING_RATE * -1 -> acquiring_out (со знаком минус)
    let acquiring_out = if finished_price > 0.0 {
        // ПРОДАЖА
        -finished_price * WB_ACQUIRING_RATE
    } else {
        // ВОЗВРАТ
        -finished_price * WB_ACQUIRING_RATE
    };

    // 8. amount_line - finished_price -> если > 0, то coinvest_in, иначе 0
    let diff = amount_line - finished_price;
    let coinvest_in = if diff > 0.0 && price_effective > 0.0 {
        diff
    } else if diff < 0.0 && price_effective < 0.0 {
        diff
    } else {
        0.0
    };

    // 9. total = amount_line + acquiring_out + commission_out
    // Разобраться

    // 10. seller_out = (customer_out + customer_in) - (acquiring_out + coinvest_in + commission_out)
    let seller_out = -(customer_out + customer_in) - (acquiring_out + coinvest_in + commission_out);
    let total = -seller_out;

    // Для реализации себестоимость пишем с минусом, для возврата - с плюсом.
    let is_return = price_effective <= 0.0;
    let resolved_cost = document
        .line
        .cost_of_production
        .filter(|price| *price > 0.0)
        .or(document.line.dealer_price_ut.filter(|price| *price > 0.0));
    let cost = normalize_cost_sign(resolved_cost, is_return);

    let entry = Model {
        id,
        registrator_ref: document_id.to_string(),
        registrator_type: "WB_Sales".to_string(),
        date: wb_business_date_str(&document.state.sale_dt),
        connection_mp_ref: document.header.connection_id.clone(),
        nomenclature_ref: document.nomenclature_ref.clone().unwrap_or_default(),
        marketplace_product_ref: document.marketplace_product_ref.clone().unwrap_or_default(),

        // Sums - рассчитанные по формулам
        customer_in,
        customer_out,
        coinvest_in,
        commission_out,
        acquiring_out,
        penalty_out: 0.0,
        logistics_out: 0.0,
        seller_out,
        price_full,
        price_list,
        price_return,
        commission_percent,
        coinvest_persent,
        total,
        cost,

        document_no: document.header.document_no.clone(),
        article: document.line.supplier_article.clone(),
        posted_at: now,
    };

    Ok(vec![entry])
}

/// Конвертировать OZON Transactions в записи Sales Data (P904)
pub async fn from_ozon_transactions(
    document: &OzonTransactions,
    document_id: &str,
) -> Result<Vec<Model>> {
    let mut entries = Vec::new();
    let now = Utc::now().to_rfc3339();

    // Если нет items, ничего не создаем
    if document.items.is_empty() {
        tracing::warn!(
            "A014 document {} has no items, skipping P904 projection",
            document.header.operation_id
        );
        return Ok(entries);
    }

    // Вычисляем logistics_total как сумму ВСЕХ сервисов
    let logistics_total: f64 = document.services.iter().map(|s| s.price).sum();

    // Получаем значения из header для распределения
    let sale_commission = document.header.sale_commission;
    let amount = document.header.amount;

    // Пытаемся найти документ отгрузки по posting_number
    let posting_number = &document.posting.posting_number;

    // Сначала пробуем найти A010 (FBS)
    let posting_fbs =
        crate::domain::a010_ozon_fbs_posting::service::get_by_document_no(posting_number).await?;

    // Если не нашли, пробуем A011 (FBO)
    let posting_fbo = if posting_fbs.is_none() {
        crate::domain::a011_ozon_fbo_posting::service::get_by_document_no(posting_number).await?
    } else {
        None
    };

    // Если не нашли ни FBS, ни FBO - логируем и создаем записи с минимальными данными
    if posting_fbs.is_none() && posting_fbo.is_none() {
        tracing::warn!(
            "A014 document {}: posting {} not found in A010/A011, creating basic P904 entries",
            document.header.operation_id,
            posting_number
        );

        // Создаем базовые записи - распределяем равномерно по items
        let items_count = document.items.len() as f64;
        let proportion = if items_count > 0.0 {
            1.0 / items_count
        } else {
            0.0
        };

        for item in &document.items {
            let sku_str = item.sku.to_string();

            // Найти/создать a007_marketplace_product
            let marketplace_product_ref =
                crate::domain::a007_marketplace_product::service::find_or_create_for_sale(
                    crate::domain::a007_marketplace_product::service::FindOrCreateParams {
                        marketplace_ref: document.header.marketplace_id.clone(),
                        connection_mp_ref: document.header.connection_id.clone(),
                        marketplace_sku: sku_str.clone(),
                        article: None,
                        barcode: None,
                        title: item.name.clone(),
                    },
                )
                .await?;

            // Получить nomenclature_ref из a007
            let nomenclature_ref = if let Some(product) =
                crate::domain::a007_marketplace_product::service::get_by_id(marketplace_product_ref)
                    .await?
            {
                product.nomenclature_ref.unwrap_or_default()
            } else {
                String::new()
            };

            // Вычисляем суммы пропорционально
            // Для возвратов (returns) accruals_for_sale идет в customer_out, иначе в customer_in
            let accruals = document.header.accruals_for_sale * proportion;
            let is_return = document.header.transaction_type == "returns";
            let (customer_in, customer_out) = if is_return {
                (0.0, accruals)
            } else {
                (accruals, 0.0)
            };
            let commission_out = sale_commission * proportion;
            let logistics_out = logistics_total * proportion;
            let seller_out = -amount * proportion;
            let total = customer_in + customer_out + commission_out + logistics_out;

            let entry = Model {
                id: Uuid::new_v4().to_string(),
                registrator_ref: document_id.to_string(),
                registrator_type: "OZON_Transactions".to_string(),
                date: document.header.operation_date.clone(),
                connection_mp_ref: document.header.connection_id.clone(),
                nomenclature_ref,
                marketplace_product_ref: marketplace_product_ref.to_string(),

                // Sums
                customer_in,
                customer_out,
                coinvest_in: 0.0,
                commission_out,
                acquiring_out: 0.0,
                penalty_out: 0.0,
                logistics_out,
                seller_out,
                price_full: 0.0,
                price_list: 0.0,
                price_return: 0.0,
                commission_percent: 0.0,
                coinvest_persent: 0.0,
                total,
                cost: None,

                document_no: posting_number.clone(),
                article: sku_str,
                posted_at: now.clone(),
            };
            entries.push(entry);
        }

        return Ok(entries);
    }

    // Работаем с найденным постингом (FBS или FBO)
    if let Some(fbs_doc) = posting_fbs {
        // FBS документ
        let total_amount: f64 = fbs_doc
            .lines
            .iter()
            .map(|l| l.amount_line.unwrap_or(0.0))
            .sum();

        for line in &fbs_doc.lines {
            // Вычисляем пропорциональную долю
            let proportion = if total_amount > 0.0 {
                line.amount_line.unwrap_or(0.0) / total_amount
            } else {
                1.0 / fbs_doc.lines.len() as f64
            };

            // Вычисляем суммы пропорционально
            // Для возвратов (returns) accruals_for_sale идет в customer_out, иначе в customer_in
            let accruals = document.header.accruals_for_sale * proportion;
            let is_return = document.header.transaction_type == "returns";
            let (customer_in, customer_out) = if is_return {
                (0.0, accruals)
            } else {
                (accruals, 0.0)
            };
            let commission_out = sale_commission * proportion;
            let logistics_out = logistics_total * proportion;
            let seller_out = -amount * proportion;
            let total = customer_in + customer_out + commission_out + logistics_out;

            // Найти/создать a007_marketplace_product
            let marketplace_product_ref =
                crate::domain::a007_marketplace_product::service::find_or_create_for_sale(
                    crate::domain::a007_marketplace_product::service::FindOrCreateParams {
                        marketplace_ref: document.header.marketplace_id.clone(),
                        connection_mp_ref: document.header.connection_id.clone(),
                        marketplace_sku: line.offer_id.clone(),
                        article: None,
                        barcode: line.barcode.clone(),
                        title: line.name.clone(),
                    },
                )
                .await?;

            // Получить nomenclature_ref из a007
            let nomenclature_ref = if let Some(product) =
                crate::domain::a007_marketplace_product::service::get_by_id(marketplace_product_ref)
                    .await?
            {
                product.nomenclature_ref.unwrap_or_default()
            } else {
                String::new()
            };

            let entry = Model {
                id: Uuid::new_v4().to_string(),
                registrator_ref: document_id.to_string(),
                registrator_type: "OZON_Transactions".to_string(),
                date: document.header.operation_date.clone(),
                connection_mp_ref: document.header.connection_id.clone(),
                nomenclature_ref,
                marketplace_product_ref: marketplace_product_ref.to_string(),

                // Sums
                customer_in,
                customer_out,
                coinvest_in: 0.0,
                commission_out,
                acquiring_out: 0.0,
                penalty_out: 0.0,
                logistics_out,
                seller_out,
                price_full: line.price_list.unwrap_or(0.0) * line.qty as f64,
                price_list: line.price_list.unwrap_or(0.0),
                price_return: 0.0,
                commission_percent: 0.0,
                coinvest_persent: 0.0,
                total,
                cost: None,

                document_no: posting_number.clone(),
                article: line.offer_id.clone(),
                posted_at: now.clone(),
            };
            entries.push(entry);
        }
    } else if let Some(fbo_doc) = posting_fbo {
        // FBO документ
        let total_amount: f64 = fbo_doc
            .lines
            .iter()
            .map(|l| l.amount_line.unwrap_or(0.0))
            .sum();

        for line in &fbo_doc.lines {
            // Вычисляем пропорциональную долю
            let proportion = if total_amount > 0.0 {
                line.amount_line.unwrap_or(0.0) / total_amount
            } else {
                1.0 / fbo_doc.lines.len() as f64
            };

            // Вычисляем суммы пропорционально
            // Для возвратов (returns) accruals_for_sale идет в customer_out, иначе в customer_in
            let accruals = document.header.accruals_for_sale * proportion;
            let is_return = document.header.transaction_type == "returns";
            let (customer_in, customer_out) = if is_return {
                (0.0, accruals)
            } else {
                (accruals, 0.0)
            };
            let commission_out = sale_commission * proportion;
            let logistics_out = logistics_total * proportion;
            let seller_out = -amount * proportion;
            let total = customer_in + customer_out + commission_out + logistics_out;

            // Найти/создать a007_marketplace_product
            let marketplace_product_ref =
                crate::domain::a007_marketplace_product::service::find_or_create_for_sale(
                    crate::domain::a007_marketplace_product::service::FindOrCreateParams {
                        marketplace_ref: document.header.marketplace_id.clone(),
                        connection_mp_ref: document.header.connection_id.clone(),
                        marketplace_sku: line.offer_id.clone(),
                        article: None,
                        barcode: line.barcode.clone(),
                        title: line.name.clone(),
                    },
                )
                .await?;

            // Получить nomenclature_ref из a007
            let nomenclature_ref = if let Some(product) =
                crate::domain::a007_marketplace_product::service::get_by_id(marketplace_product_ref)
                    .await?
            {
                product.nomenclature_ref.unwrap_or_default()
            } else {
                String::new()
            };

            let entry = Model {
                id: Uuid::new_v4().to_string(),
                registrator_ref: document_id.to_string(),
                registrator_type: "OZON_Transactions".to_string(),
                date: document.header.operation_date.clone(),
                connection_mp_ref: document.header.connection_id.clone(),
                nomenclature_ref,
                marketplace_product_ref: marketplace_product_ref.to_string(),

                // Sums
                customer_in,
                customer_out,
                coinvest_in: 0.0,
                commission_out,
                acquiring_out: 0.0,
                penalty_out: 0.0,
                logistics_out,
                seller_out,
                price_full: line.price_list.unwrap_or(0.0) * line.qty as f64,
                price_list: line.price_list.unwrap_or(0.0),
                price_return: 0.0,
                commission_percent: 0.0,
                coinvest_persent: 0.0,
                total,
                cost: None,

                document_no: posting_number.clone(),
                article: line.offer_id.clone(),
                posted_at: now.clone(),
            };
            entries.push(entry);
        }
    }

    tracing::info!(
        "Created {} P904 entries from A014 document {}",
        entries.len(),
        document.header.operation_id
    );

    Ok(entries)
}

/// Конвертировать YM Order в записи Sales Data (P904)
/// Только документы со статусом DELIVERED формируют проекции
pub async fn from_ym_order(document: &YmOrder, document_id: &str) -> Result<Vec<Model>> {
    let mut entries = Vec::new();
    let now = Utc::now().to_rfc3339();

    // Проверяем статус документа - только DELIVERED формируют проекции
    if document.state.status_norm != "DELIVERED" {
        tracing::debug!(
            "YM Order {} has status '{}', skipping P904 projection (only DELIVERED allowed)",
            document.header.document_no,
            document.state.status_norm
        );
        return Ok(entries);
    }

    // Если нет строк, ничего не создаем
    if document.lines.is_empty() {
        tracing::warn!(
            "YM Order {} has no lines, skipping P904 projection",
            document.header.document_no
        );
        return Ok(entries);
    }

    // Определяем дату для проекции (delivery_date или status_changed_at)
    let date = document
        .state
        .delivery_date
        .map(|dt| dt.to_rfc3339())
        .or_else(|| document.state.status_changed_at.map(|dt| dt.to_rfc3339()))
        .unwrap_or_else(|| Utc::now().to_rfc3339());

    let sale_date_str = date.split('T').next().unwrap_or(&date).to_string();

    for line in &document.lines {
        // customer_in берём из buyer_price или amount_line
        let customer_in = line
            .buyer_price
            .unwrap_or_else(|| line.amount_line.unwrap_or(0.0));
        let is_return = line.price_effective.unwrap_or(0.0) < 0.0
            || line.amount_line.unwrap_or(0.0) < 0.0
            || line.qty < 0.0;
        let cost = normalize_cost_sign(
            get_cost_for_nomenclature(&line.nomenclature_ref, &sale_date_str).await?,
            is_return,
        );

        let entry = Model {
            id: Uuid::new_v4().to_string(),
            registrator_ref: document_id.to_string(),
            registrator_type: "YM_Order".to_string(),
            date: date.clone(),
            connection_mp_ref: document.header.connection_id.clone(),
            nomenclature_ref: line.nomenclature_ref.clone().unwrap_or_default(),
            marketplace_product_ref: line.marketplace_product_ref.clone().unwrap_or_default(),

            // Sums - заполняем только customer_in
            customer_in,
            customer_out: 0.0,
            coinvest_in: 0.0,
            commission_out: 0.0,
            acquiring_out: 0.0,
            penalty_out: 0.0,
            logistics_out: 0.0,
            seller_out: 0.0,
            price_full: 0.0,
            price_list: line.price_list.unwrap_or(0.0),
            price_return: 0.0,
            commission_percent: 0.0,
            coinvest_persent: 0.0,
            total: customer_in,
            cost,

            document_no: document.header.document_no.clone(),
            article: line.shop_sku.clone(),
            posted_at: now.clone(),
        };
        entries.push(entry);
    }

    tracing::info!(
        "Created {} P904 entries from YM Order {} (status: {})",
        entries.len(),
        document.header.document_no,
        document.state.status_norm
    );

    Ok(entries)
}

/// Конвертировать YM Returns в записи Sales Data (P904)
/// Только документы со статусом REFUNDED формируют проекции
/// Заполняется только customer_out (с минусом - возврат денег покупателю)
pub async fn from_ym_returns(document: &YmReturn, document_id: &str) -> Result<Vec<Model>> {
    let mut entries = Vec::new();
    let now = Utc::now().to_rfc3339();

    // Проверяем статус документа - только REFUNDED формируют проекции
    if document.state.refund_status != "REFUNDED" {
        tracing::debug!(
            "YM Return {} has refund_status '{}', skipping P904 projection (only REFUNDED allowed)",
            document.header.return_id,
            document.state.refund_status
        );
        return Ok(entries);
    }

    // Если нет строк, ничего не создаем
    if document.lines.is_empty() {
        tracing::warn!(
            "YM Return {} has no lines, skipping P904 projection",
            document.header.return_id
        );
        return Ok(entries);
    }

    // Определяем дату для проекции (refund_date или created_at_source)
    let date = document
        .state
        .refund_date
        .map(|dt| dt.to_rfc3339())
        .or_else(|| document.state.created_at_source.map(|dt| dt.to_rfc3339()))
        .unwrap_or_else(|| Utc::now().to_rfc3339());

    for line in &document.lines {
        // Определяем сумму возврата:
        // 1. Ищем решение с типом REFUND_MONEY
        // 2. Если не нашли - используем price * count
        let refund_amount = line
            .decisions
            .iter()
            .find(|d| d.decision_type == "REFUND_MONEY")
            .and_then(|d| d.amount)
            .unwrap_or_else(|| line.price.unwrap_or(0.0) * line.count as f64);

        // customer_out отрицательный (возврат денег покупателю)
        let customer_out = -refund_amount;

        let entry = Model {
            id: Uuid::new_v4().to_string(),
            registrator_ref: document_id.to_string(),
            registrator_type: "YM_Returns".to_string(),
            date: date.clone(),
            connection_mp_ref: document.header.connection_id.clone(),
            nomenclature_ref: String::new(), // Пока не заполняем
            marketplace_product_ref: String::new(), // Пока не заполняем

            // Sums - заполняем только customer_out (с минусом)
            customer_in: 0.0,
            customer_out,
            coinvest_in: 0.0,
            commission_out: 0.0,
            acquiring_out: 0.0,
            penalty_out: 0.0,
            logistics_out: 0.0,
            seller_out: 0.0,
            price_full: 0.0,
            price_list: line.price.unwrap_or(0.0),
            price_return: refund_amount,
            commission_percent: 0.0,
            coinvest_persent: 0.0,
            total: customer_out,
            cost: None,

            document_no: format!("YM-RET-{}", document.header.return_id),
            article: line.shop_sku.clone(),
            posted_at: now.clone(),
        };
        entries.push(entry);
    }

    tracing::info!(
        "Created {} P904 entries from YM Return {} (refund_status: {})",
        entries.len(),
        document.header.return_id,
        document.state.refund_status
    );

    Ok(entries)
}

/// Конвертировать OZON FBS Posting в записи Sales Data (P904)
/// Только документы со статусом DELIVERED формируют проекции
pub async fn from_ozon_fbs(document: &OzonFbsPosting, document_id: &str) -> Result<Vec<Model>> {
    let mut entries = Vec::new();
    let now = Utc::now().to_rfc3339();

    // Проверяем статус документа - только DELIVERED формируют проекции
    if document.state.status_norm != "DELIVERED" {
        tracing::debug!(
            "OZON FBS Posting {} has status '{}', skipping P904 projection (only DELIVERED allowed)",
            document.header.document_no,
            document.state.status_norm
        );
        return Ok(entries);
    }

    // Если нет строк, ничего не создаем
    if document.lines.is_empty() {
        tracing::warn!(
            "OZON FBS Posting {} has no lines, skipping P904 projection",
            document.header.document_no
        );
        return Ok(entries);
    }

    // Определяем дату для проекции (delivered_at или updated_at_source)
    let date = document
        .state
        .delivered_at
        .map(|dt| dt.to_rfc3339())
        .or_else(|| document.state.updated_at_source.map(|dt| dt.to_rfc3339()))
        .unwrap_or_else(|| Utc::now().to_rfc3339());

    for line in &document.lines {
        // customer_in берём из amount_line
        let customer_in = line.amount_line.unwrap_or(0.0);

        // Найти/создать a007_marketplace_product
        let marketplace_product_ref =
            crate::domain::a007_marketplace_product::service::find_or_create_for_sale(
                crate::domain::a007_marketplace_product::service::FindOrCreateParams {
                    marketplace_ref: document.header.marketplace_id.clone(),
                    connection_mp_ref: document.header.connection_id.clone(),
                    marketplace_sku: line.offer_id.clone(),
                    article: None,
                    barcode: line.barcode.clone(),
                    title: line.name.clone(),
                },
            )
            .await?;

        // Получить nomenclature_ref из a007
        let nomenclature_ref = if let Some(product) =
            crate::domain::a007_marketplace_product::service::get_by_id(marketplace_product_ref)
                .await?
        {
            product.nomenclature_ref.unwrap_or_default()
        } else {
            String::new()
        };

        let entry = Model {
            id: Uuid::new_v4().to_string(),
            registrator_ref: document_id.to_string(),
            registrator_type: "OZON_FBS".to_string(),
            date: date.clone(),
            connection_mp_ref: document.header.connection_id.clone(),
            nomenclature_ref,
            marketplace_product_ref: marketplace_product_ref.to_string(),

            // Sums - заполняем только customer_in
            customer_in,
            customer_out: 0.0,
            coinvest_in: 0.0,
            commission_out: 0.0,
            acquiring_out: 0.0,
            penalty_out: 0.0,
            logistics_out: 0.0,
            seller_out: 0.0,
            price_full: line.price_list.unwrap_or(0.0) * line.qty as f64,
            price_list: line.price_list.unwrap_or(0.0),
            price_return: 0.0,
            commission_percent: 0.0,
            coinvest_persent: 0.0,
            total: customer_in,
            cost: None,

            document_no: document.header.document_no.clone(),
            article: line.offer_id.clone(),
            posted_at: now.clone(),
        };
        entries.push(entry);
    }

    tracing::info!(
        "Created {} P904 entries from OZON FBS Posting {} (status: {})",
        entries.len(),
        document.header.document_no,
        document.state.status_norm
    );

    Ok(entries)
}

/// Конвертировать OZON FBO Posting в записи Sales Data (P904)
/// Документы формируют проекции независимо от статуса
pub async fn from_ozon_fbo(document: &OzonFboPosting, document_id: &str) -> Result<Vec<Model>> {
    let mut entries = Vec::new();
    let now = Utc::now().to_rfc3339();

    // Если нет строк, ничего не создаем
    if document.lines.is_empty() {
        tracing::warn!(
            "OZON FBO Posting {} has no lines, skipping P904 projection",
            document.header.document_no
        );
        return Ok(entries);
    }

    // Определяем дату для проекции (delivered_at или updated_at_source или created_at)
    let date = document
        .state
        .delivered_at
        .map(|dt| dt.to_rfc3339())
        .or_else(|| document.state.updated_at_source.map(|dt| dt.to_rfc3339()))
        .or_else(|| document.state.created_at.map(|dt| dt.to_rfc3339()))
        .unwrap_or_else(|| Utc::now().to_rfc3339());

    for line in &document.lines {
        // customer_in берём из amount_line
        let customer_in = line.amount_line.unwrap_or(0.0);

        // Найти/создать a007_marketplace_product
        let marketplace_product_ref =
            crate::domain::a007_marketplace_product::service::find_or_create_for_sale(
                crate::domain::a007_marketplace_product::service::FindOrCreateParams {
                    marketplace_ref: document.header.marketplace_id.clone(),
                    connection_mp_ref: document.header.connection_id.clone(),
                    marketplace_sku: line.offer_id.clone(),
                    article: None,
                    barcode: line.barcode.clone(),
                    title: line.name.clone(),
                },
            )
            .await?;

        // Получить nomenclature_ref из a007
        let nomenclature_ref = if let Some(product) =
            crate::domain::a007_marketplace_product::service::get_by_id(marketplace_product_ref)
                .await?
        {
            product.nomenclature_ref.unwrap_or_default()
        } else {
            String::new()
        };

        let entry = Model {
            id: Uuid::new_v4().to_string(),
            registrator_ref: document_id.to_string(),
            registrator_type: "OZON_FBO".to_string(),
            date: date.clone(),
            connection_mp_ref: document.header.connection_id.clone(),
            nomenclature_ref,
            marketplace_product_ref: marketplace_product_ref.to_string(),

            // Sums - заполняем только customer_in
            customer_in,
            customer_out: 0.0,
            coinvest_in: 0.0,
            commission_out: 0.0,
            acquiring_out: 0.0,
            penalty_out: 0.0,
            logistics_out: 0.0,
            seller_out: 0.0,
            price_full: line.price_list.unwrap_or(0.0) * line.qty as f64,
            price_list: line.price_list.unwrap_or(0.0),
            price_return: 0.0,
            commission_percent: 0.0,
            coinvest_persent: 0.0,
            total: customer_in,
            cost: None,

            document_no: document.header.document_no.clone(),
            article: line.offer_id.clone(),
            posted_at: now.clone(),
        };
        entries.push(entry);
    }

    tracing::info!(
        "Created {} P904 entries from OZON FBO Posting {}",
        entries.len(),
        document.header.document_no
    );

    Ok(entries)
}

/// Конвертировать OZON Returns в записи Sales Data (P904)
/// Возвраты формируют проекции с отрицательным customer_out
pub async fn from_ozon_returns(document: &OzonReturns, document_id: &str) -> Result<Vec<Model>> {
    let mut entries = Vec::new();
    let now = Utc::now().to_rfc3339();

    // Дата возврата
    let date = document
        .return_date
        .and_hms_opt(0, 0, 0)
        .unwrap_or_else(|| chrono::Utc::now().naive_utc())
        .and_utc()
        .to_rfc3339();

    // Сумма возврата (с минусом)
    let return_amount = document.price * document.quantity as f64;
    let customer_out = -return_amount;

    // Найти/создать a007_marketplace_product
    let marketplace_product_ref =
        crate::domain::a007_marketplace_product::service::find_or_create_for_sale(
            crate::domain::a007_marketplace_product::service::FindOrCreateParams {
                marketplace_ref: document.marketplace_id.clone(),
                connection_mp_ref: document.connection_id.clone(),
                marketplace_sku: document.sku.clone(),
                article: None,
                barcode: None,
                title: document.product_name.clone(),
            },
        )
        .await?;

    // Получить nomenclature_ref из a007
    let nomenclature_ref = if let Some(product) =
        crate::domain::a007_marketplace_product::service::get_by_id(marketplace_product_ref).await?
    {
        product.nomenclature_ref.unwrap_or_default()
    } else {
        String::new()
    };

    let entry = Model {
        id: Uuid::new_v4().to_string(),
        registrator_ref: document_id.to_string(),
        registrator_type: "OZON_Returns".to_string(),
        date,
        connection_mp_ref: document.connection_id.clone(),
        nomenclature_ref,
        marketplace_product_ref: marketplace_product_ref.to_string(),

        // Sums - заполняем только customer_out (с минусом - возврат денег покупателю)
        customer_in: 0.0,
        customer_out,
        coinvest_in: 0.0,
        commission_out: 0.0,
        acquiring_out: 0.0,
        penalty_out: 0.0,
        logistics_out: 0.0,
        seller_out: 0.0,
        price_full: 0.0,
        price_list: document.price,
        price_return: return_amount,
        commission_percent: 0.0,
        coinvest_persent: 0.0,
        total: customer_out,
        cost: None,

        document_no: document.return_id.clone(),
        article: document.sku.clone(),
        posted_at: now,
    };
    entries.push(entry);

    tracing::info!(
        "Created {} P904 entries from OZON Return {} (qty: {})",
        entries.len(),
        document.return_id,
        document.quantity
    );

    Ok(entries)
}
