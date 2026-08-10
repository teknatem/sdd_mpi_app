//! Парсер «Отчёта о реализации» YM → документы a034_ym_realization.
//!
//! Архив отчёта многофайловый. Для бухгалтерской реализации берём:
//!   - `delivered.csv` — доставлено (выручка, признаётся на дату доставки);
//!   - `returned.csv`  — возвраты (уменьшают выручку).
//! Прочие файлы (transferred_to_delivery / unredeemed / lost_items) — это
//! отгрузка/невыкуп/потери, в выручку реализации не входят.
//!
//! Один документ a034 = кабинет × день; строки — по SKU. Дата привязывается к
//! месяцу отчёта (clamp): дата вне [month_first, month_last] или пустая → конец
//! месяца, чтобы весь месячный отчёт попадал в свой месяц (как акт реализации).

use anyhow::Result;
use contracts::domain::a006_connection_mp::aggregate::ConnectionMP;
use contracts::domain::a034_ym_realization::aggregate::{
    YmRealization, YmRealizationHeader, YmRealizationLine, YmRealizationSourceMeta,
};
use contracts::domain::common::AggregateId;
use std::collections::BTreeMap;

use super::payment_report::{parse_decimal, ru_date_to_iso};

/// Описание одного файла отчёта: имена колонок-кандидатов (калибруются под
/// реальную выгрузку — у delivered/returned разные имена колонок суммы).
struct FileSpec {
    /// Окончание имени файла в архиве.
    filename: &'static str,
    /// true — строки возврата (уменьшают выручку).
    is_return: bool,
    /// Колонки даты по приоритету (пер-строчный fallback).
    date: &'static [&'static str],
    /// Колонка номера заказа (общий ключ сверки с p907).
    order: &'static [&'static str],
    shop_sku: &'static [&'static str],
    /// Артикул продавца (YOUR_SKU) — для резолва позиции в a007.
    your_sku: &'static [&'static str],
    offer_name: &'static [&'static str],
    quantity: &'static [&'static str],
    /// Колонка суммы с НДС и всеми скидками (выручка по цене покупателя).
    revenue: &'static [&'static str],
}

const SPECS: &[FileSpec] = &[
    // Доставлено — выручка реализации.
    FileSpec {
        filename: "delivered.csv",
        is_return: false,
        date: &[
            "DELIVERY_DATE",
            "TRANSFERRED_TO_DELIVERY_DATE",
            "ORDER_CREATION_DATE",
        ],
        order: &["ORDER_ID"],
        shop_sku: &["SHOP_SKU", "YOUR_SKU"],
        your_sku: &["YOUR_SKU"],
        offer_name: &["OFFER_NAME"],
        quantity: &["DELIVERED_COUNT", "TRANSFERRED_TO_DELIVERY_COUNT"],
        revenue: &["DELIVERED_PRICE_SUM_WITH_VAT_AND_DISCOUNTS"],
    },
    // Возвраты — уменьшают выручку.
    FileSpec {
        filename: "returned.csv",
        is_return: true,
        date: &[
            "RETURN_WAREHOUSE_OR_SC_ACCEPT_DATE",
            "DELIVERY_DATE",
            "ORDER_CREATION_DATE",
        ],
        order: &["ORDER_ID"],
        shop_sku: &["SHOP_SKU", "YOUR_SKU"],
        your_sku: &["YOUR_SKU"],
        offer_name: &["OFFER_NAME"],
        quantity: &["RETURNED_COUNT", "DELIVERED_COUNT"],
        revenue: &["RETURN_PRICE_SUM_WITH_VAT_AND_DISCOUNTS"],
    },
];

pub struct ParsedRealization {
    pub documents: Vec<YmRealization>,
    pub skipped: i32,
}

/// Строки одного дня, разнесённые по типу (продажи / возвраты).
#[derive(Default)]
struct DayLines {
    sales: Vec<YmRealizationLine>,
    returns: Vec<YmRealizationLine>,
}

/// Очистка денежной строки YM: убираем пробелы/неразрывные пробелы (разделители
/// тысяч), затем парсим с учётом запятой как десятичного разделителя.
fn parse_money(raw: &str) -> Option<f64> {
    let cleaned: String = raw
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '\u{00a0}')
        .collect();
    parse_decimal(&cleaned)
}

/// Привязка даты строки к месяцу отчёта: вне диапазона или пустая → конец месяца.
fn clamp_to_month(day: Option<String>, month_first: &str, month_last: &str) -> String {
    match day {
        Some(d) if d.as_str() >= month_first && d.as_str() <= month_last => d,
        _ => month_last.to_string(),
    }
}

/// Разбирает все CSV-файлы отчёта о реализации в документы a034 (день × кабинет ×
/// кампания). Отчёт YM запрашивается на кампанию, поэтому все строки одного вызова
/// относятся к переданным `campaign_id` / `placement_type` (модель FBS/FBY/…).
/// `month_first`/`month_last` — границы месяца отчёта (YYYY-MM-DD) для clamp дат.
pub fn parse_realization_files(
    connection: &ConnectionMP,
    organization_id: &str,
    files: &[(String, String)],
    month_first: &str,
    month_last: &str,
    campaign_id: Option<&str>,
    placement_type: Option<&str>,
) -> Result<ParsedRealization> {
    let connection_id = connection.base.id.as_string();
    let marketplace_id = connection.marketplace_id.clone();
    let fetched_at = chrono::Utc::now().to_rfc3339();

    let mut skipped = 0i32;
    // Продажи и возвраты разносятся в отдельные коллекции уже на парсинге —
    // приходят из разных файлов (delivered/returned) и не смешиваются.
    let mut by_day: BTreeMap<String, DayLines> = BTreeMap::new();

    for spec in SPECS {
        let Some((_, content)) = files
            .iter()
            .find(|(name, _)| name.to_lowercase().ends_with(spec.filename))
        else {
            tracing::warn!("Realization ZIP: файл {} не найден, пропуск", spec.filename);
            continue;
        };
        parse_file(
            spec,
            content,
            month_first,
            month_last,
            &mut by_day,
            &mut skipped,
        );
    }

    let mut documents = Vec::with_capacity(by_day.len());
    for (day, lines) in by_day {
        // document_no несёт кампанию, чтобы документы разных моделей (FBS/FBY)
        // одного дня не сталкивались по номеру. Без кампании — legacy-формат.
        let document_no = match campaign_id {
            Some(cid) if !cid.is_empty() => format!("YMREAL-{}-{}-{}", connection_id, cid, day),
            _ => format!("YMREAL-{}-{}", connection_id, day),
        };
        let header = YmRealizationHeader {
            document_no,
            document_date: day,
            connection_id: connection_id.clone(),
            organization_id: organization_id.to_string(),
            marketplace_id: marketplace_id.clone(),
            campaign_id: campaign_id.filter(|s| !s.is_empty()).map(|s| s.to_string()),
            placement_type: placement_type
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string()),
        };
        let mut document = YmRealization::new_for_insert(
            header,
            lines.sales,
            lines.returns,
            YmRealizationSourceMeta {
                source: "ym_goods_realization".to_string(),
                fetched_at: fetched_at.clone(),
            },
        );
        document.is_posted = true;
        document.base.metadata.is_posted = true;
        documents.push(document);
    }

    tracing::info!(
        "Realization parsed: {} day-documents, {} skipped rows",
        documents.len(),
        skipped
    );
    Ok(ParsedRealization { documents, skipped })
}

fn parse_file(
    spec: &FileSpec,
    csv_text: &str,
    month_first: &str,
    month_last: &str,
    by_day: &mut BTreeMap<String, DayLines>,
    skipped: &mut i32,
) {
    let text = csv_text.trim_start_matches('\u{FEFF}');
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .from_reader(text.as_bytes());

    let headers = match reader.headers() {
        Ok(h) => h.clone(),
        Err(e) => {
            tracing::warn!(
                "Realization {}: не прочитать заголовки: {}",
                spec.filename,
                e
            );
            return;
        }
    };

    let idx_of = |names: &[&str]| -> Option<usize> {
        names.iter().find_map(|name| {
            headers
                .iter()
                .position(|h| h.trim().eq_ignore_ascii_case(name))
        })
    };
    let date_indices: Vec<usize> = spec
        .date
        .iter()
        .filter_map(|name| {
            headers
                .iter()
                .position(|h| h.trim().eq_ignore_ascii_case(name))
        })
        .collect();
    let idx_order = idx_of(spec.order);
    let idx_your_sku = idx_of(spec.your_sku);
    let idx_shop_sku = idx_of(spec.shop_sku);
    let idx_offer_name = idx_of(spec.offer_name);
    let idx_quantity = idx_of(spec.quantity);
    let idx_revenue = idx_of(spec.revenue);

    for result in reader.records() {
        let record = match result {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("Realization {}: битая строка: {}", spec.filename, e);
                *skipped += 1;
                continue;
            }
        };
        let get = |idx: Option<usize>| -> Option<String> {
            idx.and_then(|i| record.get(i))
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        };

        let row_day = date_indices.iter().find_map(|&i| {
            record
                .get(i)
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(ru_date_to_iso)
                .filter(|iso| iso.len() >= 10)
                .map(|iso| iso.chars().take(10).collect::<String>())
        });
        let day = clamp_to_month(row_day, month_first, month_last);

        let revenue = get(idx_revenue)
            .and_then(|v| parse_money(&v))
            .unwrap_or(0.0);

        let line = YmRealizationLine {
            order_id: get(idx_order),
            shop_sku: get(idx_shop_sku).unwrap_or_default(),
            your_sku: get(idx_your_sku),
            marketplace_product_ref: None,
            market_sku: None,
            offer_name: get(idx_offer_name).unwrap_or_default(),
            quantity: get(idx_quantity)
                .and_then(|v| parse_money(&v))
                .unwrap_or(0.0),
            // Выручка хранится положительной; знак операции несёт is_return.
            revenue_amount: revenue.abs(),
            is_return: spec.is_return,
        };
        let bucket = by_day.entry(day).or_default();
        if spec.is_return {
            bucket.returns.push(line);
        } else {
            bucket.sales.push(line);
        }
    }
}
