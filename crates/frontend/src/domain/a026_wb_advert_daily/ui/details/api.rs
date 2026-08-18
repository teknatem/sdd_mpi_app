//! API layer for WB Advert Daily details
//!
//! Contains DTOs, formatters/helpers and async API functions for fetching and
//! mutating WB Advert Daily data.

use crate::general_ledger::api::fetch_document_general_ledger_entries;
use crate::shared::api_utils::api_base;
use crate::shared::export::ExcelExportable;
use crate::shared::list_utils::Sortable;
use contracts::domain::a026_wb_advert_daily::aggregate::WbAdvertDailyMetrics;
use contracts::general_ledger::GeneralLedgerEntryDto;
use gloo_net::http::Request;
use serde::Deserialize;
use std::cmp::Ordering;

// ============================================
// Table identifiers / constants
// ============================================

pub const LINES_TABLE_ID: &str = "a026-wb-advert-daily-lines-table";
pub const LINES_COLUMN_WIDTHS_KEY: &str = "a026_wb_advert_daily_details_lines_column_widths";
pub const LINKED_ORDERS_TABLE_ID: &str = "a026-wb-advert-daily-linked-orders-table";
pub const LINKED_ORDERS_COLUMN_WIDTHS_KEY: &str =
    "a026_wb_advert_daily_details_linked_orders_column_widths";

/// Минимальный расход для отображения строки в таблице атрибуции (1 коп.).
pub const MIN_ALLOCATED_COST_DISPLAY: f64 = 0.01;

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

pub fn fmt_expense_share(expense: f64, total_expense: f64) -> String {
    if total_expense.abs() <= f64::EPSILON {
        "—".to_string()
    } else {
        fmt_ratio(expense / total_expense * 100.0)
    }
}

pub fn should_show_linked_order(order: &FoundOrderDto) -> bool {
    !order.is_allocated || order.allocated_cost.abs() >= MIN_ALLOCATED_COST_DISPLAY
}

pub fn should_show_linked_group(group: &LinkedOrdersByNmDto) -> bool {
    if group.wb_reported_orders > 0 && group.found_orders.is_empty() {
        return true;
    }
    if group.wb_advert_sum.abs() >= MIN_ALLOCATED_COST_DISPLAY {
        return true;
    }
    group.found_orders.iter().any(should_show_linked_order)
}

pub fn fmt_csv_decimal(v: f64) -> String {
    format!("{:.2}", v).replace('.', ",")
}

pub fn fmt_advert_id(advert_id: i64) -> String {
    if advert_id > 0 {
        advert_id.to_string()
    } else {
        "—".to_string()
    }
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
    pub wb_name: String,
    #[serde(default)]
    pub marketplace_product_ref: Option<String>,
    pub nomenclature_ref: Option<String>,
    pub nomenclature_article: Option<String>,
    pub metrics: WbAdvertDailyMetrics,
}

impl Sortable for LineDto {
    fn compare_by_field(&self, other: &Self, field: &str) -> Ordering {
        match field {
            "nm_id" => self.nm_id.cmp(&other.nm_id),
            "wb_name" => cmp_text(&self.wb_name, &other.wb_name),
            "nomenclature_article" => {
                cmp_optional_text(&self.nomenclature_article, &other.nomenclature_article)
            }
            "views" => self.metrics.views.cmp(&other.metrics.views),
            "clicks" => self.metrics.clicks.cmp(&other.metrics.clicks),
            "ctr" => cmp_float(self.metrics.ctr, other.metrics.ctr),
            "cpc" => cmp_float(self.metrics.cpc, other.metrics.cpc),
            "atbs" => self.metrics.atbs.cmp(&other.metrics.atbs),
            "orders" => self.metrics.orders.cmp(&other.metrics.orders),
            "shks" => self.metrics.shks.cmp(&other.metrics.shks),
            "sum" => cmp_float(self.metrics.sum, other.metrics.sum),
            "sum_price" => cmp_float(self.metrics.sum_price, other.metrics.sum_price),
            "cr" => cmp_float(self.metrics.cr, other.metrics.cr),
            "canceled" => self.metrics.canceled.cmp(&other.metrics.canceled),
            _ => Ordering::Equal,
        }
    }
}

impl ExcelExportable for LineDto {
    fn headers() -> Vec<&'static str> {
        vec![
            "nmID",
            "WB наименование",
            "Артикул 1С",
            "Просмотры",
            "Клики",
            "CTR, %",
            "CPC",
            "В корзину",
            "Заказы",
            "Штуки",
            "Расход",
            "Выручка",
            "CR, %",
            "Отмены",
        ]
    }

    fn to_csv_row(&self) -> Vec<String> {
        vec![
            self.nm_id.to_string(),
            self.wb_name.clone(),
            self.nomenclature_article
                .clone()
                .unwrap_or_else(|| "—".to_string()),
            self.metrics.views.to_string(),
            self.metrics.clicks.to_string(),
            fmt_csv_decimal(self.metrics.ctr),
            fmt_csv_decimal(self.metrics.cpc),
            self.metrics.atbs.to_string(),
            self.metrics.orders.to_string(),
            self.metrics.shks.to_string(),
            fmt_csv_decimal(self.metrics.sum),
            fmt_csv_decimal(self.metrics.sum_price),
            fmt_csv_decimal(self.metrics.cr),
            self.metrics.canceled.to_string(),
        ]
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct FoundOrderDto {
    pub order_key: String,
    #[serde(default)]
    pub order_id: Option<String>,
    #[serde(default)]
    pub order_date: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    pub nomenclature_ref: Option<String>,
    #[serde(default)]
    pub finished_price: Option<f64>,
    #[serde(default)]
    pub is_cancel: bool,
    #[serde(default)]
    pub allocation_basis: f64,
    #[serde(default)]
    pub is_allocated: bool,
    #[serde(default)]
    #[allow(dead_code)]
    pub allocation_ratio: f64,
    #[serde(default)]
    pub allocated_cost: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LinkedOrdersByNmDto {
    pub nm_id: i64,
    pub nm_name: String,
    #[serde(default)]
    pub nomenclature_ref: Option<String>,
    #[serde(default)]
    pub nomenclature_article: Option<String>,
    #[serde(default)]
    pub wb_reported_orders: i64,
    #[serde(default)]
    pub wb_advert_sum: f64,
    #[serde(default)]
    pub found_orders: Vec<FoundOrderDto>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DetailsDto {
    pub id: String,
    pub document_no: String,
    pub document_date: String,
    #[serde(default)]
    pub advert_id: i64,
    pub connection_id: String,
    pub connection_name: Option<String>,
    pub organization_id: String,
    pub organization_name: Option<String>,
    pub marketplace_id: String,
    pub marketplace_name: Option<String>,
    pub totals: WbAdvertDailyMetrics,
    pub unattributed_totals: WbAdvertDailyMetrics,
    pub source: String,
    pub fetched_at: String,
    pub created_at: String,
    pub updated_at: String,
    pub is_posted: bool,
    pub lines: Vec<LineDto>,
    #[serde(default)]
    pub has_linked_orders: bool,
    #[serde(default)]
    pub linked_orders_count: i64,
    #[serde(default)]
    pub linked_orders: Vec<LinkedOrdersByNmDto>,
}

// ============================================
// API Functions
// ============================================

/// Fetch WB Advert Daily detail by ID
pub async fn fetch_by_id(id: &str) -> Result<DetailsDto, String> {
    let url = format!("{}/api/a026/wb-advert-daily/{}", api_base(), id);

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

/// Fetch projections for a WB Advert Daily document (p913 + p911) as raw JSON.
pub async fn fetch_projections(id: &str) -> Result<serde_json::Value, String> {
    let url = format!("{}/api/a026/wb-advert-daily/{}/projections", api_base(), id);

    let response = Request::get(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch projections: {}", e))?;

    if !response.ok() {
        return Err(format!("Server error: {}", response.status()));
    }

    let text = response
        .text()
        .await
        .map_err(|e| format!("Failed to read response: {}", e))?;

    serde_json::from_str(&text).map_err(|e| format!("Failed to parse projections: {}", e))
}

/// Post (проведение) document
pub async fn post_document(id: &str) -> Result<(), String> {
    let url = format!("{}/api/a026/wb-advert-daily/{}/post", api_base(), id);

    let response = Request::post(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to post document: {}", e))?;

    if !response.ok() {
        return Err(format!("Failed to post: status {}", response.status()));
    }

    Ok(())
}

/// Unpost (отмена проведения) document
pub async fn unpost_document(id: &str) -> Result<(), String> {
    let url = format!("{}/api/a026/wb-advert-daily/{}/unpost", api_base(), id);

    let response = Request::post(&url)
        .send()
        .await
        .map_err(|e| format!("Failed to unpost document: {}", e))?;

    if !response.ok() {
        return Err(format!("Failed to unpost: status {}", response.status()));
    }

    Ok(())
}

/// Fetch journal (General Ledger) entries for a WB Advert Daily document.
pub async fn fetch_general_ledger_entries(id: &str) -> Result<Vec<GeneralLedgerEntryDto>, String> {
    fetch_document_general_ledger_entries("a026_wb_advert_daily", id).await
}
