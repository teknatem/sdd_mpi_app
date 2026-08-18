//! API layer for WB Sales Funnel Daily details
//!
//! Contains DTOs, formatters/helpers and async API functions.

use crate::shared::api_utils::api_base;
use crate::shared::export::ExcelExportable;
use crate::shared::list_utils::Sortable;
use contracts::domain::a036_wb_sales_funnel_daily::aggregate::WbSalesFunnelDailyMetrics;
use gloo_net::http::Request;
use serde::Deserialize;
use std::cmp::Ordering;

// ============================================
// Table identifiers / constants
// ============================================

pub const LINES_TABLE_ID: &str = "a036-wb-sales-funnel-lines-table";
pub const LINES_COLUMN_WIDTHS_KEY: &str = "a036_wb_sales_funnel_daily_details_lines_column_widths";

// ============================================
// Formatters & helpers
// ============================================

pub fn fmt_date(v: &str) -> String {
    if let Some((y, rest)) = v.split_once('-') {
        if let Some((m, d)) = rest.split_once('-') {
            return format!("{}.{}.{}", d, m, y);
        }
    }
    v.to_string()
}

pub fn fmt_dt(v: &str) -> String {
    if let Some((d, t)) = v.split_once('T') {
        return format!(
            "{} {}",
            fmt_date(d),
            t.split(['Z', '+', '.']).next().unwrap_or(t)
        );
    }
    fmt_date(v)
}

pub fn fmt_money(v: f64) -> String {
    format!("{:.2}", v)
}

pub fn fmt_ratio(v: f64) -> String {
    format!("{:.2}", v)
}

pub fn fmt_csv_decimal(v: f64) -> String {
    format!("{:.2}", v).replace('.', ",")
}

fn cmp_text(a: &str, b: &str) -> Ordering {
    a.to_lowercase().cmp(&b.to_lowercase())
}

fn cmp_optional_text(a: &Option<String>, b: &Option<String>) -> Ordering {
    match (a, b) {
        (Some(a), Some(b)) => cmp_text(a, b),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

fn cmp_float(a: f64, b: f64) -> Ordering {
    a.partial_cmp(&b).unwrap_or(Ordering::Equal)
}

// ============================================
// DTOs
// ============================================

#[derive(Debug, Clone, Deserialize)]
pub struct LineDto {
    pub nm_id: i64,
    pub title: String,
    pub vendor_code: String,
    pub brand_name: String,
    #[serde(default)]
    #[allow(dead_code)]
    pub subject_id: i64,
    pub subject_name: String,
    #[serde(default)]
    pub marketplace_product_ref: Option<String>,
    pub nomenclature_ref: Option<String>,
    pub nomenclature_article: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    pub nomenclature_name: Option<String>,
    pub metrics: WbSalesFunnelDailyMetrics,
}

impl Sortable for LineDto {
    fn compare_by_field(&self, other: &Self, field: &str) -> Ordering {
        match field {
            "nm_id" => self.nm_id.cmp(&other.nm_id),
            "title" => cmp_text(&self.title, &other.title),
            "vendor_code" => cmp_text(&self.vendor_code, &other.vendor_code),
            "brand_name" => cmp_text(&self.brand_name, &other.brand_name),
            "subject_name" => cmp_text(&self.subject_name, &other.subject_name),
            "nomenclature_article" => {
                cmp_optional_text(&self.nomenclature_article, &other.nomenclature_article)
            }
            "open_count" => self.metrics.open_count.cmp(&other.metrics.open_count),
            "cart_count" => self.metrics.cart_count.cmp(&other.metrics.cart_count),
            "order_count" => self.metrics.order_count.cmp(&other.metrics.order_count),
            "order_sum" => cmp_float(self.metrics.order_sum, other.metrics.order_sum),
            "buyout_count" => self.metrics.buyout_count.cmp(&other.metrics.buyout_count),
            "buyout_sum" => cmp_float(self.metrics.buyout_sum, other.metrics.buyout_sum),
            "buyout_percent" => {
                cmp_float(self.metrics.buyout_percent, other.metrics.buyout_percent)
            }
            "cancel_count" => self.metrics.cancel_count.cmp(&other.metrics.cancel_count),
            "cancel_sum" => cmp_float(
                self.metrics.cancel_sum.unwrap_or(0.0),
                other.metrics.cancel_sum.unwrap_or(0.0),
            ),
            "add_to_cart_conversion" => cmp_float(
                self.metrics.add_to_cart_conversion,
                other.metrics.add_to_cart_conversion,
            ),
            "cart_to_order_conversion" => cmp_float(
                self.metrics.cart_to_order_conversion,
                other.metrics.cart_to_order_conversion,
            ),
            "add_to_wishlist_count" => self
                .metrics
                .add_to_wishlist_count
                .cmp(&other.metrics.add_to_wishlist_count),
            _ => Ordering::Equal,
        }
    }
}

impl ExcelExportable for LineDto {
    fn headers() -> Vec<&'static str> {
        vec![
            "nmID",
            "Наименование",
            "Артикул продавца",
            "Бренд",
            "Предмет",
            "Артикул 1С",
            "Переходы",
            "В корзину",
            "Конв. в корзину, %",
            "Заказы",
            "Конв. в заказ, %",
            "Сумма заказов",
            "Выкупы",
            "Сумма выкупов",
            "Процент выкупа, %",
            "Отмены",
            "Сумма отмен",
            "Отложенные",
        ]
    }

    fn to_csv_row(&self) -> Vec<String> {
        vec![
            self.nm_id.to_string(),
            self.title.clone(),
            self.vendor_code.clone(),
            self.brand_name.clone(),
            self.subject_name.clone(),
            self.nomenclature_article
                .clone()
                .unwrap_or_else(|| "—".to_string()),
            self.metrics.open_count.to_string(),
            self.metrics.cart_count.to_string(),
            fmt_csv_decimal(self.metrics.add_to_cart_conversion),
            self.metrics.order_count.to_string(),
            fmt_csv_decimal(self.metrics.cart_to_order_conversion),
            fmt_csv_decimal(self.metrics.order_sum),
            self.metrics.buyout_count.to_string(),
            fmt_csv_decimal(self.metrics.buyout_sum),
            fmt_csv_decimal(self.metrics.buyout_percent),
            // Нет счётчика в источнике → пустая ячейка, а не 0.
            self.metrics
                .cancel_count
                .map(|v| v.to_string())
                .unwrap_or_default(),
            self.metrics
                .cancel_sum
                .map(fmt_csv_decimal)
                .unwrap_or_default(),
            self.metrics.add_to_wishlist_count.to_string(),
        ]
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct DetailsDto {
    pub id: String,
    pub document_no: String,
    pub document_date: String,
    pub connection_id: String,
    pub connection_name: Option<String>,
    pub organization_id: String,
    pub organization_name: Option<String>,
    pub marketplace_id: String,
    pub marketplace_name: Option<String>,
    #[serde(default)]
    pub currency: String,
    pub totals: WbSalesFunnelDailyMetrics,
    pub source: String,
    pub fetched_at: String,
    pub created_at: String,
    pub updated_at: String,
    pub lines: Vec<LineDto>,
}

// ============================================
// API Functions
// ============================================

/// Fetch WB Sales Funnel Daily detail by ID
pub async fn fetch_by_id(id: &str) -> Result<DetailsDto, String> {
    let url = format!("{}/api/a036/wb-sales-funnel/{}", api_base(), id);

    let response = Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("Ошибка сети: {}", e))?;

    if !response.ok() {
        return Err(format!("Ошибка сервера: HTTP {}", response.status()));
    }

    response
        .json::<DetailsDto>()
        .await
        .map_err(|e| format!("Ошибка парсинга: {}", e))
}

/// Провести документ: пересобрать его движения воронки p916.
pub async fn post_document(id: &str) -> Result<(), String> {
    let url = format!("{}/api/a036/wb-sales-funnel/{}/post", api_base(), id);
    let response = Request::post(&url)
        .send()
        .await
        .map_err(|e| format!("Ошибка сети: {}", e))?;

    if !response.ok() {
        return Err(format!("Ошибка сервера: HTTP {}", response.status()));
    }
    Ok(())
}

/// Движения проекции p916, порождённые документом a036, как raw JSON (закладка «Проекции»).
pub async fn fetch_projections(id: &str) -> Result<serde_json::Value, String> {
    let url = format!("{}/api/a036/wb-sales-funnel/{}/projections", api_base(), id);

    let response = Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("Ошибка сети: {}", e))?;

    if !response.ok() {
        return Err(format!("Ошибка сервера: HTTP {}", response.status()));
    }

    response
        .json::<serde_json::Value>()
        .await
        .map_err(|e| format!("Ошибка парсинга: {}", e))
}
