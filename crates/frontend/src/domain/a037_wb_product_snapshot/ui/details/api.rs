//! API layer for WB Product Snapshot details.

use crate::shared::api_utils::api_base;
use crate::shared::export::ExcelExportable;
use crate::shared::list_utils::Sortable;
use contracts::domain::a037_wb_product_snapshot::aggregate::WbProductSnapshotState;
use gloo_net::http::Request;
use serde::Deserialize;
use std::cmp::Ordering;

// ============================================
// Table identifiers / constants
// ============================================

pub const LINES_TABLE_ID: &str = "a037-wb-product-snapshot-lines-table";
// v2: набор колонок изменился (убрана «Динамика», добавлены ch-кап числовых) —
// новый ключ сбрасывает устаревшие сохранённые ширины из localStorage.
pub const LINES_COLUMN_WIDTHS_KEY: &str = "a037_wb_product_snapshot_details_lines_column_widths_v2";

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
    crate::shared::components::table::number_format::format_number_with_decimals(v, 2)
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
    #[serde(default)]
    #[allow(dead_code)]
    pub subject_name: String,
    #[serde(default)]
    pub marketplace_product_ref: Option<String>,
    pub nomenclature_article: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    pub nomenclature_name: Option<String>,
    pub state: WbProductSnapshotState,
}

impl Sortable for LineDto {
    fn compare_by_field(&self, other: &Self, field: &str) -> Ordering {
        match field {
            "nm_id" => self.nm_id.cmp(&other.nm_id),
            "title" => cmp_text(&self.title, &other.title),
            "vendor_code" => cmp_text(&self.vendor_code, &other.vendor_code),
            "brand_name" => cmp_text(&self.brand_name, &other.brand_name),
            "nomenclature_article" => {
                cmp_optional_text(&self.nomenclature_article, &other.nomenclature_article)
            }
            "stock_wb" => self.state.stock_wb.cmp(&other.state.stock_wb),
            "stock_mp" => self.state.stock_mp.cmp(&other.state.stock_mp),
            "stock_balance_sum" => {
                cmp_float(self.state.stock_balance_sum, other.state.stock_balance_sum)
            }
            "product_rating" => cmp_float(self.state.product_rating, other.state.product_rating),
            "feedback_rating" => cmp_float(self.state.feedback_rating, other.state.feedback_rating),
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
            "Артикул 1С",
            "Остаток WB",
            "Остаток продавца",
            "Сумма остатков",
            "Рейтинг карточки",
            "Оценка покупателей",
        ]
    }

    fn to_csv_row(&self) -> Vec<String> {
        vec![
            self.nm_id.to_string(),
            self.title.clone(),
            self.vendor_code.clone(),
            self.brand_name.clone(),
            self.nomenclature_article
                .clone()
                .unwrap_or_else(|| "—".to_string()),
            self.state.stock_wb.to_string(),
            self.state.stock_mp.to_string(),
            fmt_csv_decimal(self.state.stock_balance_sum),
            fmt_csv_decimal(self.state.product_rating),
            fmt_csv_decimal(self.state.feedback_rating),
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
    pub total_stock_wb: i64,
    pub total_stock_mp: i64,
    pub total_balance_sum: f64,
    pub source: String,
    pub fetched_at: String,
    pub created_at: String,
    pub updated_at: String,
    pub lines: Vec<LineDto>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SeriesPointDto {
    pub date: String,
    pub stock_wb: i64,
    pub stock_mp: i64,
    #[allow(dead_code)]
    pub stock_balance_sum: f64,
    pub product_rating: f64,
    pub feedback_rating: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SeriesResponse {
    #[allow(dead_code)]
    pub nm_id: i64,
    pub points: Vec<SeriesPointDto>,
}

// ============================================
// API Functions
// ============================================

/// Fetch WB Product Snapshot detail by ID.
pub async fn fetch_by_id(id: &str) -> Result<DetailsDto, String> {
    let url = format!("{}/api/a037/wb-product-snapshot/{}", api_base(), id);
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

/// Fetch per-product dynamics series (динамика по дням).
pub async fn fetch_series(
    connection_id: &str,
    nm_id: i64,
    date_from: &str,
    date_to: &str,
) -> Result<Vec<SeriesPointDto>, String> {
    let url = format!(
        "{}/api/a037/wb-product-snapshot/series?connection_id={}&nm_id={}&date_from={}&date_to={}",
        api_base(),
        urlencoding::encode(connection_id),
        nm_id,
        urlencoding::encode(date_from),
        urlencoding::encode(date_to),
    );
    let response = Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("Ошибка сети: {}", e))?;
    if !response.ok() {
        return Err(format!("Ошибка сервера: HTTP {}", response.status()));
    }
    response
        .json::<SeriesResponse>()
        .await
        .map(|r| r.points)
        .map_err(|e| format!("Ошибка парсинга: {}", e))
}

// ============================================
// Воронка (a036): переходы / в корзину / заказы за N дней
// ============================================

#[derive(Debug, Clone, Deserialize)]
pub struct ProductMetricsDto {
    pub nm_id: i64,
    pub open_count: i64,
    pub cart_count: i64,
    pub order_count: i64,
}

/// Сумма метрик воронки a036 по nm_id за период [date_from, date_to] для кабинета.
pub async fn fetch_product_metrics(
    connection_id: &str,
    date_from: &str,
    date_to: &str,
) -> Result<Vec<ProductMetricsDto>, String> {
    let url = format!(
        "{}/api/a036/wb-sales-funnel/product-metrics?connection_id={}&date_from={}&date_to={}",
        api_base(),
        urlencoding::encode(connection_id),
        urlencoding::encode(date_from),
        urlencoding::encode(date_to),
    );
    let response = Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("Ошибка сети: {}", e))?;
    if !response.ok() {
        return Err(format!("Ошибка сервера: HTTP {}", response.status()));
    }
    response
        .json::<Vec<ProductMetricsDto>>()
        .await
        .map_err(|e| format!("Ошибка парсинга: {}", e))
}

// ============================================
// Изменения рейтинга/оценки vs предыдущий снимок
// ============================================

#[derive(Debug, Clone, Deserialize)]
pub struct RatingChangeDto {
    pub nm_id: i64,
    pub title: String,
    pub vendor_code: String,
    #[serde(default)]
    #[allow(dead_code)]
    pub brand_name: String,
    pub nomenclature_article: Option<String>,
    pub marketplace_product_ref: Option<String>,
    pub product_rating_old: f64,
    pub product_rating_new: f64,
    pub product_rating_delta: f64,
    pub feedback_rating_old: f64,
    pub feedback_rating_new: f64,
    pub feedback_rating_delta: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RatingChangesResponse {
    pub has_previous: bool,
    pub prev_date: Option<String>,
    #[allow(dead_code)]
    pub prev_document_no: Option<String>,
    pub rows: Vec<RatingChangeDto>,
}

/// Загрузка позиций с изменившимся рейтингом/оценкой относительно прошлого снимка.
pub async fn fetch_rating_changes(id: &str) -> Result<RatingChangesResponse, String> {
    let url = format!(
        "{}/api/a037/wb-product-snapshot/rating-changes?id={}",
        api_base(),
        urlencoding::encode(id),
    );
    let response = Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("Ошибка сети: {}", e))?;
    if !response.ok() {
        return Err(format!("Ошибка сервера: HTTP {}", response.status()));
    }
    response
        .json::<RatingChangesResponse>()
        .await
        .map_err(|e| format!("Ошибка парсинга: {}", e))
}
