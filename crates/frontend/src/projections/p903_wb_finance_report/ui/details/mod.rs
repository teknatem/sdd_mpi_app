mod api;

use crate::domain::a012_wb_sales::ui::details::WbSalesDetail;
use crate::general_ledger::ui::{
    document_general_ledger_entries_nav_id, DocumentGeneralLedgerEntries,
};
use crate::layout::global_context::AppGlobalContext;
use crate::shared::icons::icon;
use crate::shared::json_viewer::widget::JsonViewer;
use crate::shared::list_utils::{format_number, get_sort_class, get_sort_indicator};
use crate::shared::page_frame::PageFrame;
use crate::shared::table_utils::init_column_resize;
use api::{fetch_detail, fetch_linked_sales, post_detail, WbSalesLink};
use contracts::general_ledger::GeneralLedgerEntryDto;
use contracts::projections::p903_wb_finance_report::dto::WbFinanceReportDto;
use leptos::logging::log;
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde_json::Value;
use thaw::*;
use wasm_bindgen::JsCast;

#[derive(Debug, Clone)]
struct FieldRow {
    description: String,
    field_id: String,
    value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GlFieldRole {
    Condition,
    Resource,
    ResourceAndCondition,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FinancialResultRole {
    Income,
    Expense,
    Info,
}

const EXCLUDED_PAYMENT_PROCESSING_VALUE: &str = "Комиссия за организацию платежа с НДС";

const FIELDS_TABLE_ID: &str = "p903-fields-table";
const FIELDS_COLUMN_WIDTHS_KEY: &str = "p903_fields_column_widths";

/// Колонки списка полей: ключ сортировки, заголовок, стартовая ширина.
const FIELD_COLUMNS: [(&str, &str, &str); 5] = [
    ("description", "Описание", "auto"),
    ("gl_role", "Роль", "150px"),
    ("financial_result", "Результат", "120px"),
    ("field_id", "Идентификатор", "220px"),
    ("value", "Значение", "200px"),
];

fn extra_string_field(item: &WbFinanceReportDto, field: &str) -> Option<String> {
    item.extra
        .as_deref()
        .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
        .and_then(|json| {
            json.get(field)
                .and_then(|value| value.as_str())
                .map(|value| value.trim().to_string())
        })
        .filter(|value| !value.is_empty())
}

fn extra_f64_field(item: &WbFinanceReportDto, field: &str) -> Option<f64> {
    item.extra
        .as_deref()
        .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
        .and_then(|json| {
            json.get(field).and_then(|value| {
                value.as_f64().or_else(|| {
                    value
                        .as_str()
                        .and_then(|raw| raw.trim().parse::<f64>().ok())
                })
            })
        })
}

fn field_note(field_id: &str) -> Option<&'static str> {
    match field_id {
        "supplier_oper_name" => Some(
            "Правило GL: поле выбирает ветку posting для customer_revenue/customer_return, mp_commission/mp_commission_adjustment, mp_storage, mp_penalty/mp_penalty_storno, voluntary_return_compensation и mp_ppvz_reward.",
        ),
        "srid" => Some(
            "Правило GL: непустой SRID делает строку linked. Это влияет на customer_revenue/customer_return, знаки расходов и posting acceptance только для строк без SRID.",
        ),
        "retail_amount" => Some(
            "Правило GL: ресурс для customer_revenue по linked sale-строке; если у возврата пустой return_amount, retail_amount используется как fallback для customer_return. Также участвует в определении sale-строки.",
        ),
        "return_amount" => Some(
            "Правило GL: ресурс для customer_return, если поле заполнено. Также участвует в определении return-строки и знака для mp_acquiring, mp_commission и mp_ppvz_reward.",
        ),
        "ppvz_vw" => Some(
            "Правило GL: часть комиссии WB. Для sale/return используется сумма ppvz_vw + ppvz_vw_nds в turnover mp_commission; для прочих операций вместе с ppvz_sales_commission формирует mp_commission_adjustment.",
        ),
        "ppvz_vw_nds" => Some(
            "Правило GL: часть комиссии WB. Для sale/return используется сумма ppvz_vw + ppvz_vw_nds в turnover mp_commission; для прочих операций вместе с ppvz_sales_commission формирует mp_commission_adjustment.",
        ),
        "ppvz_sales_commission" => Some(
            "Правило GL: участвует только в turnover mp_commission_adjustment для не sale/non-return операций; в sale/return ветке это поле в ресурс не входит.",
        ),
        "acquiring_fee" => Some(
            "Правило GL: ресурс для mp_acquiring. Для linked-возвратов сумма разворачивается в минус; строки с payment_processing = 'Комиссия за организацию платежа с НДС' исключаются.",
        ),
        "rebill_logistic_cost" => Some(
            "Правило GL: проводка формируется только по rebill_logistic_cost в turnover mp_rebill_logistic_cost.",
        ),
        "ppvz_reward" => Some(
            "Правило GL: источник — raw WB JSON field extra.ppvz_reward. Знак: Продажа = плюс, Возврат = минус, операция 'Возмещение за выдачу и возврат товаров на ПВЗ' = плюс. Turnover: mp_ppvz_reward.",
        ),
        "storage_fee" => Some(
            "Правило GL: ресурс для mp_storage только при supplier_oper_name = 'Хранение'. Знак зависит от linked/unlinked ветки.",
        ),
        "penalty" => Some(
            "Правило GL: ресурс для mp_penalty / mp_penalty_storno только при supplier_oper_name = 'Штраф'. Положительная сумма идет в mp_penalty, отрицательная — в mp_penalty_storno.",
        ),
        "ppvz_for_pay" => Some(
            "Правило GL: ресурс для voluntary_return_compensation только при supplier_oper_name = 'Добровольная компенсация при возврате'.",
        ),
        "delivery_amount" => {
            Some("Правило GL: ресурс для acceptance только для unlinked-строк без SRID.")
        }
        "payment_processing" => Some(
            "Значение показывается из raw WB JSON. Для GL mp_acquiring значение 'Комиссия за организацию платежа с НДС' исключается.",
        ),
        _ => None,
    }
}

fn is_emphasized_string_field(field_id: &str) -> bool {
    matches!(field_id, "payment_processing")
}

/// Производные поля, добавленные системой (не приходят из WB API напрямую).
/// В UI выделяются серой рамкой слева, чтобы отличаться от оригинальных полей.
fn is_derived_field(field_id: &str) -> bool {
    matches!(
        field_id,
        "id" | "source_row_ref"
            | "general_ledger_entries_count"
            | "a004_nomenclature_ref"
            | "marketplace_product_ref"
            | "marketplace_order_ref"
    )
}

fn display_field_note(field_id: &str) -> Option<&'static str> {
    match field_id {
        _ => field_note(field_id),
    }
}

fn display_field_description(row: &FieldRow) -> String {
    match row.field_id.as_str() {
        "payment_processing" => "Тип обработки платежа".to_string(),
        "ppvz_reward" => "Возмещение за выдачу и возврат товаров на ПВЗ".to_string(),
        _ => row.description.clone(),
    }
}

fn gl_resource_turnovers(field_id: &str) -> &'static [&'static str] {
    match field_id {
        "retail_amount" => &["customer_revenue", "customer_revenue_storno"],
        "return_amount" => &["customer_return", "customer_revenue_storno"],
        "ppvz_vw" | "ppvz_vw_nds" => &[
            "mp_commission",
            "mp_commission_storno",
            "mp_commission_adjustment",
        ],
        "ppvz_sales_commission" => &["mp_commission_adjustment"],
        "acquiring_fee" => &["mp_acquiring", "mp_acquiring_storno"],
        "rebill_logistic_cost" => &["mp_rebill_logistic_cost"],
        "ppvz_reward" => &["mp_ppvz_reward"],
        "storage_fee" => &["mp_storage"],
        "penalty" => &["mp_penalty", "mp_penalty_storno"],
        "ppvz_for_pay" => &["voluntary_return_compensation"],
        "delivery_amount" => &["acceptance"],
        _ => &[],
    }
}

fn gl_condition_turnovers(field_id: &str) -> &'static [&'static str] {
    match field_id {
        "supplier_oper_name" => &[
            "customer_revenue",
            "customer_return",
            "customer_revenue_storno",
            "mp_commission",
            "mp_commission_storno",
            "mp_commission_adjustment",
            "mp_acquiring",
            "mp_acquiring_storno",
            "mp_storage",
            "mp_penalty",
            "mp_penalty_storno",
            "voluntary_return_compensation",
            "mp_ppvz_reward",
        ],
        "srid" => &[
            "customer_revenue",
            "customer_return",
            "customer_revenue_storno",
            "mp_commission",
            "mp_commission_storno",
            "mp_commission_adjustment",
            "mp_acquiring",
            "mp_acquiring_storno",
            "mp_rebill_logistic_cost",
            "mp_storage",
            "acceptance",
        ],
        "payment_processing" => &["mp_acquiring", "mp_acquiring_storno"],
        "retail_amount" => &[
            "customer_revenue",
            "customer_return",
            "customer_revenue_storno",
            "mp_commission",
            "mp_commission_storno",
            "mp_ppvz_reward",
        ],
        "return_amount" => &[
            "customer_return",
            "customer_revenue_storno",
            "mp_acquiring",
            "mp_acquiring_storno",
            "mp_commission",
            "mp_commission_storno",
            "mp_ppvz_reward",
        ],
        _ => &[],
    }
}

fn has_turnover(entries: &[GeneralLedgerEntryDto], turnover_code: &str) -> bool {
    entries
        .iter()
        .any(|entry| entry.turnover_code == turnover_code)
}

fn field_gl_role(field_id: &str, entries: &[GeneralLedgerEntryDto]) -> Option<GlFieldRole> {
    let is_resource = gl_resource_turnovers(field_id)
        .iter()
        .any(|turnover_code| has_turnover(entries, turnover_code));
    let is_condition = gl_condition_turnovers(field_id)
        .iter()
        .any(|turnover_code| has_turnover(entries, turnover_code));

    match (is_resource, is_condition) {
        (true, true) => Some(GlFieldRole::ResourceAndCondition),
        (true, false) => Some(GlFieldRole::Resource),
        (false, true) => Some(GlFieldRole::Condition),
        (false, false) => None,
    }
}

fn gl_role_badge_label(role: GlFieldRole) -> &'static str {
    match role {
        GlFieldRole::Condition => "Условие",
        GlFieldRole::Resource => "Ресурс",
        GlFieldRole::ResourceAndCondition => "Ресурс + условие",
    }
}

fn gl_role_badge_class(role: GlFieldRole) -> &'static str {
    match role {
        GlFieldRole::Condition => "badge badge--accent",
        GlFieldRole::Resource => "badge badge--primary",
        GlFieldRole::ResourceAndCondition => "badge badge--warning",
    }
}

/// Цвет полосы категории слева от строки. Отдаётся в разметку рантайм-переменной
/// `--spec-cat`, чтобы не заводить модификатор `spec-list__row--*` под каждую роль.
/// Берутся насыщенные токены темы, а не приглушённые бордеры бейджей: полоса —
/// единственная метка категории в строке и должна читаться с расстояния.
fn gl_role_cat_color(role: Option<GlFieldRole>, derived: bool) -> &'static str {
    match role {
        Some(GlFieldRole::Condition) => "var(--color-accent)",
        Some(GlFieldRole::Resource) => "var(--color-primary)",
        Some(GlFieldRole::ResourceAndCondition) => "var(--color-warning)",
        // Производные поля: добавлены системой, в ответе WB API их нет.
        None if derived => "var(--color-border)",
        None => "transparent",
    }
}

fn gl_role_sort_label(role: Option<GlFieldRole>) -> &'static str {
    match role {
        Some(role) => gl_role_badge_label(role),
        None => "—",
    }
}

fn financial_result_badge_label(role: FinancialResultRole) -> &'static str {
    match role {
        FinancialResultRole::Income => "Доход",
        FinancialResultRole::Expense => "Расход",
        FinancialResultRole::Info => "info",
    }
}

fn financial_result_sort_order(role: FinancialResultRole) -> u8 {
    match role {
        FinancialResultRole::Income => 0,
        FinancialResultRole::Expense => 1,
        FinancialResultRole::Info => 2,
    }
}

fn financial_result_badge_class(role: FinancialResultRole) -> &'static str {
    match role {
        FinancialResultRole::Income => "badge badge--success",
        FinancialResultRole::Expense => "badge badge--error",
        FinancialResultRole::Info => "badge badge--neutral",
    }
}

fn parse_field_value(value: &str) -> Option<f64> {
    value
        .trim()
        .replace(' ', "")
        .replace(',', ".")
        .parse::<f64>()
        .ok()
        .filter(|value| value.abs() > f64::EPSILON)
}

fn turnover_financial_result(turnover_code: &str) -> Option<FinancialResultRole> {
    match turnover_code {
        "customer_revenue"
        | "voluntary_return_compensation"
        | "mp_penalty_storno"
        | "mp_commission_storno"
        | "mp_acquiring_storno" => Some(FinancialResultRole::Income),
        "customer_return"
        | "customer_revenue_storno"
        | "mp_commission"
        | "mp_commission_adjustment"
        | "mp_commission_adjustment_nm"
        | "mp_acquiring"
        | "mp_logistics"
        | "mp_rebill_logistic_cost"
        | "mp_rebill_logistic_cost_nm"
        | "mp_ppvz_reward"
        | "mp_ppvz_reward_nm"
        | "mp_storage"
        | "acceptance"
        | "mp_penalty" => Some(FinancialResultRole::Expense),
        _ => None,
    }
}

fn field_financial_result_role(
    row: &FieldRow,
    entries: &[GeneralLedgerEntryDto],
) -> FinancialResultRole {
    for turnover_code in gl_resource_turnovers(&row.field_id) {
        if has_turnover(entries, turnover_code) {
            if let Some(role) = turnover_financial_result(turnover_code) {
                return role;
            }
        }
    }

    match row.field_id.as_str() {
        "additional_payment" => match parse_field_value(&row.value) {
            Some(value) if value > 0.0 => FinancialResultRole::Income,
            Some(_) => FinancialResultRole::Expense,
            None => FinancialResultRole::Info,
        },
        "cashback_amount" => {
            if parse_field_value(&row.value).is_some() {
                FinancialResultRole::Expense
            } else {
                FinancialResultRole::Info
            }
        }
        _ => FinancialResultRole::Info,
    }
}

/// Категория для чипов-фильтров над списком.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CategoryFilter {
    All,
    Condition,
    Resource,
    Both,
    Other,
}

impl CategoryFilter {
    fn label(self) -> &'static str {
        match self {
            CategoryFilter::All => "Все",
            CategoryFilter::Condition => "Условие",
            CategoryFilter::Resource => "Ресурс",
            CategoryFilter::Both => "Ресурс + условие",
            CategoryFilter::Other => "Прочее",
        }
    }

    fn accepts(self, role: Option<GlFieldRole>) -> bool {
        match self {
            CategoryFilter::All => true,
            CategoryFilter::Condition => role == Some(GlFieldRole::Condition),
            CategoryFilter::Resource => role == Some(GlFieldRole::Resource),
            CategoryFilter::Both => role == Some(GlFieldRole::ResourceAndCondition),
            CategoryFilter::Other => role.is_none(),
        }
    }
}

const CATEGORY_FILTERS: [CategoryFilter; 5] = [
    CategoryFilter::All,
    CategoryFilter::Condition,
    CategoryFilter::Resource,
    CategoryFilter::Both,
    CategoryFilter::Other,
];

/// Строка списка со всем, что нужно и для отрисовки, и для поиска, и для выгрузки.
/// Собирается один раз, чтобы фильтр, таблица и CSV работали по одному набору.
#[derive(Debug, Clone)]
struct FieldView {
    description: String,
    note: Option<&'static str>,
    field_id: String,
    value: String,
    gl_role: Option<GlFieldRole>,
    result_role: FinancialResultRole,
    derived: bool,
    emphasized: bool,
}

impl FieldView {
    fn new(row: &FieldRow, entries: &[GeneralLedgerEntryDto]) -> Self {
        Self {
            description: display_field_description(row),
            note: display_field_note(&row.field_id),
            field_id: row.field_id.clone(),
            value: row.value.clone(),
            gl_role: field_gl_role(&row.field_id, entries),
            result_role: field_financial_result_role(row, entries),
            derived: is_derived_field(&row.field_id),
            emphasized: is_emphasized_string_field(&row.field_id),
        }
    }

    /// Текст, по которому идёт быстрый поиск. Описание попадает в него только в
    /// подробном режиме — ищем ровно по тому, что видно на экране.
    fn haystack(&self, compact: bool) -> String {
        let mut text = format!(
            "{} {} {} {} {}",
            self.description,
            self.field_id,
            self.value,
            gl_role_sort_label(self.gl_role),
            financial_result_badge_label(self.result_role),
        );
        if !compact {
            if let Some(note) = self.note {
                text.push(' ');
                text.push_str(note);
            }
        }
        text.to_lowercase()
    }

    /// Вкладка, которую открывает значение-ссылка.
    fn link_target(&self) -> Option<(String, String)> {
        let has_value = self.value != "-" && !self.value.is_empty();
        if !has_value {
            return None;
        }
        let short = &self.value[..self.value.len().min(8)];
        match self.field_id.as_str() {
            "marketplace_product_ref" => Some((
                format!("a007_marketplace_product_details_{}", self.value),
                format!("Товар {short}"),
            )),
            "marketplace_order_ref" => Some((
                format!("a015_wb_orders_details_{}", self.value),
                format!("Заказ {short}"),
            )),
            _ => None,
        }
    }
}

#[component]
pub fn WbFinanceReportDetail(id: String, #[prop(into)] on_close: Callback<()>) -> impl IntoView {
    let tabs_store = use_context::<AppGlobalContext>().expect("AppGlobalContext not found");
    let (data, set_data) = signal::<Option<WbFinanceReportDto>>(None);
    let (general_ledger_entries, set_general_ledger_entries) =
        signal::<Vec<GeneralLedgerEntryDto>>(Vec::new());
    let (loading, set_loading) = signal(true);
    let (posting, set_posting) = signal(false);
    let (error, set_error) = signal(None::<String>);
    let (action_message, set_action_message) = signal(None::<String>);
    let (active_tab, set_active_tab) = signal("fields");
    let (sort_by, set_sort_by) = signal("description".to_string());
    let (sort_desc, set_sort_desc) = signal(false);

    // Тулбар списка полей: краткий/подробный режим, поиск, фильтр по категории.
    let compact = RwSignal::new(false);
    let query = RwSignal::new(String::new());
    let category = RwSignal::new(CategoryFilter::All);

    // Linked sales documents
    let (linked_sales, set_linked_sales) = signal::<Vec<WbSalesLink>>(Vec::new());
    let (links_loading, set_links_loading) = signal(false);
    let (links_error, set_links_error) = signal(None::<String>);
    let (links_fetched, set_links_fetched) = signal(false);
    let (selected_sale_id, set_selected_sale_id) = signal::<Option<String>>(None);

    // Загрузка данных
    let id_clone = id.clone();
    Effect::new(move || {
        let id = id_clone.clone();

        spawn_local(async move {
            match fetch_detail(&id).await {
                Ok(response) => {
                    set_action_message.set(None);
                    set_general_ledger_entries.set(response.general_ledger_entries);
                    set_data.set(Some(response.item));
                    set_loading.set(false);
                }
                Err(e) => {
                    log!("Failed to fetch finance report detail: {:?}", e);
                    set_error.set(Some(e));
                    set_loading.set(false);
                }
            }
        });
    });

    // Загрузка связанных документов продаж при активации вкладки Links (однократно)
    Effect::new(move || {
        let tab = active_tab.get();
        let item = data.get();
        if tab == "links" && !links_fetched.get_untracked() {
            if let Some(item) = item {
                if let Some(srid_val) = item.srid {
                    if !srid_val.is_empty() {
                        set_links_fetched.set(true);
                        set_links_loading.set(true);
                        set_links_error.set(None);

                        spawn_local(async move {
                            match fetch_linked_sales(&srid_val).await {
                                Ok(sales) => {
                                    set_linked_sales.set(sales);
                                    set_links_loading.set(false);
                                }
                                Err(e) => {
                                    log!("Failed to fetch linked sales: {:?}", e);
                                    set_links_error.set(Some(e));
                                    set_links_loading.set(false);
                                }
                            }
                        });
                    }
                }
            }
        }
    });

    // Преобразование данных в таблицу полей
    let get_field_rows = move || -> Vec<FieldRow> {
        let Some(item) = data.get() else {
            return Vec::new();
        };

        let mut rows = vec![
            FieldRow {
                description: "Эквайринг/Комиссии за организацию платежей".to_string(),
                field_id: "acquiring_fee".to_string(),
                value: item
                    .acquiring_fee
                    .map(|v| format!("{:.2}", v))
                    .unwrap_or_else(|| "-".to_string()),
            },
            FieldRow {
                description: "Размер комиссии за эквайринг/Комиссии за организацию платежей, %"
                    .to_string(),
                field_id: "acquiring_percent".to_string(),
                value: item
                    .acquiring_percent
                    .map(|v| format!("{:.2}", v))
                    .unwrap_or_else(|| "-".to_string()),
            },
            FieldRow {
                description: "Корректировка Вознаграждения Вайлдберриз (ВВ)".to_string(),
                field_id: "additional_payment".to_string(),
                value: item
                    .additional_payment
                    .map(|v| format!("{:.2}", v))
                    .unwrap_or_else(|| "-".to_string()),
            },
            FieldRow {
                description: "Виды логистики, штрафов и корректировок ВВ".to_string(),
                field_id: "bonus_type_name".to_string(),
                value: item
                    .bonus_type_name
                    .clone()
                    .unwrap_or_else(|| "-".to_string()),
            },
            FieldRow {
                description: "Размер кВВ, %".to_string(),
                field_id: "commission_percent".to_string(),
                value: item
                    .commission_percent
                    .map(|v| format!("{:.2}", v))
                    .unwrap_or_else(|| "-".to_string()),
            },
            FieldRow {
                description: "Количество доставок".to_string(),
                field_id: "delivery_amount".to_string(),
                value: item
                    .delivery_amount
                    .map(|v| format!("{:.0}", v))
                    .unwrap_or_else(|| "-".to_string()),
            },
            FieldRow {
                description: "Услуги по доставке товара покупателю".to_string(),
                field_id: "delivery_rub".to_string(),
                value: item
                    .delivery_rub
                    .map(|v| format!("{:.2}", v))
                    .unwrap_or_else(|| "-".to_string()),
            },
            FieldRow {
                description: "Артикул WB".to_string(),
                field_id: "nm_id".to_string(),
                value: item
                    .nm_id
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "-".to_string()),
            },
            FieldRow {
                description: "Общая сумма штрафов".to_string(),
                field_id: "penalty".to_string(),
                value: item
                    .penalty
                    .map(|v| format!("{:.2}", v))
                    .unwrap_or_else(|| "-".to_string()),
            },
            FieldRow {
                description: "Вознаграждение Вайлдберриз (ВВ), без НДС".to_string(),
                field_id: "ppvz_vw".to_string(),
                value: item
                    .ppvz_vw
                    .map(|v| format!("{:.2}", v))
                    .unwrap_or_else(|| "-".to_string()),
            },
            FieldRow {
                description: "НДС с вознаграждения Вайлдберриз".to_string(),
                field_id: "ppvz_vw_nds".to_string(),
                value: item
                    .ppvz_vw_nds
                    .map(|v| format!("{:.2}", v))
                    .unwrap_or_else(|| "-".to_string()),
            },
            FieldRow {
                description: "Вознаграждение с продаж до вычета услуг поверенного, без НДС"
                    .to_string(),
                field_id: "ppvz_sales_commission".to_string(),
                value: item
                    .ppvz_sales_commission
                    .map(|v| format!("{:.2}", v))
                    .unwrap_or_else(|| "-".to_string()),
            },
            FieldRow {
                description: "Возмещение за выдачу и возврат товаров на ПВЗ (raw JSON)".to_string(),
                field_id: "ppvz_reward".to_string(),
                value: extra_f64_field(&item, "ppvz_reward")
                    .map(|v| format!("{:.2}", v))
                    .unwrap_or_else(|| "-".to_string()),
            },
            FieldRow {
                description: "Количество".to_string(),
                field_id: "quantity".to_string(),
                value: item
                    .quantity
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "-".to_string()),
            },
            FieldRow {
                description: "Возмещение издержек по перевозке/по складским операциям с товаром"
                    .to_string(),
                field_id: "rebill_logistic_cost".to_string(),
                value: item
                    .rebill_logistic_cost
                    .map(|v| format!("{:.2}", v))
                    .unwrap_or_else(|| "-".to_string()),
            },
            FieldRow {
                description: "Вайлдберриз реализовал Товар (Пр)".to_string(),
                field_id: "retail_amount".to_string(),
                value: item
                    .retail_amount
                    .map(|v| format!("{:.2}", v))
                    .unwrap_or_else(|| "-".to_string()),
            },
            FieldRow {
                description: "Цена розничная".to_string(),
                field_id: "retail_price".to_string(),
                value: item
                    .retail_price
                    .map(|v| format!("{:.2}", v))
                    .unwrap_or_else(|| "-".to_string()),
            },
            FieldRow {
                description: "Цена розничная с учётом согласованной скидки".to_string(),
                field_id: "retail_price_withdisc_rub".to_string(),
                value: item
                    .retail_price_withdisc_rub
                    .map(|v| format!("{:.2}", v))
                    .unwrap_or_else(|| "-".to_string()),
            },
            FieldRow {
                description: "Количество возврата".to_string(),
                field_id: "return_amount".to_string(),
                value: item
                    .return_amount
                    .map(|v| format!("{:.0}", v))
                    .unwrap_or_else(|| "-".to_string()),
            },
            FieldRow {
                description: "Дата операции".to_string(),
                field_id: "rr_dt".to_string(),
                value: item.rr_dt.clone(),
            },
            FieldRow {
                description: "Артикул продавца".to_string(),
                field_id: "sa_name".to_string(),
                value: item.sa_name.clone().unwrap_or_else(|| "-".to_string()),
            },
            FieldRow {
                description: "Хранение".to_string(),
                field_id: "storage_fee".to_string(),
                value: item
                    .storage_fee
                    .map(|v| format!("{:.2}", v))
                    .unwrap_or_else(|| "-".to_string()),
            },
            FieldRow {
                description: "Предмет".to_string(),
                field_id: "subject_name".to_string(),
                value: item.subject_name.clone().unwrap_or_else(|| "-".to_string()),
            },
            FieldRow {
                description: "Обоснование для оплаты".to_string(),
                field_id: "supplier_oper_name".to_string(),
                value: item
                    .supplier_oper_name
                    .clone()
                    .unwrap_or_else(|| "-".to_string()),
            },
            FieldRow {
                description: "Сумма, удержанная за начисленные баллы программы лояльности"
                    .to_string(),
                field_id: "cashback_amount".to_string(),
                value: item
                    .cashback_amount
                    .map(|v| format!("{:.2}", v))
                    .unwrap_or_else(|| "-".to_string()),
            },
            FieldRow {
                description: "К перечислению продавцу за реализованный товар".to_string(),
                field_id: "ppvz_for_pay".to_string(),
                value: item
                    .ppvz_for_pay
                    .map(|v| format!("{:.2}", v))
                    .unwrap_or_else(|| "-".to_string()),
            },
            FieldRow {
                description: "Итоговый кВВ без НДС, %".to_string(),
                field_id: "ppvz_kvw_prc".to_string(),
                value: item
                    .ppvz_kvw_prc
                    .map(|v| format!("{:.2}", v))
                    .unwrap_or_else(|| "-".to_string()),
            },
            FieldRow {
                description: "Размер кВВ без НДС, % базовый".to_string(),
                field_id: "ppvz_kvw_prc_base".to_string(),
                value: item
                    .ppvz_kvw_prc_base
                    .map(|v| format!("{:.2}", v))
                    .unwrap_or_else(|| "-".to_string()),
            },
            FieldRow {
                description: "Признак услуги платной доставки".to_string(),
                field_id: "srv_dbs".to_string(),
                value: item
                    .srv_dbs
                    .map(|v| {
                        if v == 1 {
                            "Да".to_string()
                        } else {
                            "Нет".to_string()
                        }
                    })
                    .unwrap_or_else(|| "-".to_string()),
            },
            FieldRow {
                description: "SRID (Уникальный идентификатор строки)".to_string(),
                field_id: "srid".to_string(),
                value: item.srid.clone().unwrap_or_else(|| "-".to_string()),
            },
            FieldRow {
                description: "ID".to_string(),
                field_id: "id".to_string(),
                value: item.id.clone(),
            },
            FieldRow {
                description: "Source row reference".to_string(),
                field_id: "source_row_ref".to_string(),
                value: item.source_row_ref.clone(),
            },
            FieldRow {
                description: "General ledger entries count".to_string(),
                field_id: "general_ledger_entries_count".to_string(),
                value: item.general_ledger_entries_count.to_string(),
            },
            FieldRow {
                description: "Номенклатура (a004)".to_string(),
                field_id: "a004_nomenclature_ref".to_string(),
                value: item
                    .a004_nomenclature_ref
                    .clone()
                    .unwrap_or_else(|| "-".to_string()),
            },
            FieldRow {
                description: "Товар маркетплейса (a007)".to_string(),
                field_id: "marketplace_product_ref".to_string(),
                value: item
                    .marketplace_product_ref
                    .clone()
                    .unwrap_or_else(|| "-".to_string()),
            },
            FieldRow {
                description: "Заказ (a015)".to_string(),
                field_id: "marketplace_order_ref".to_string(),
                value: item
                    .marketplace_order_ref
                    .clone()
                    .unwrap_or_else(|| "-".to_string()),
            },
        ];

        // Сортировка
        rows.push(FieldRow {
            description: format!(
                "Тип обработки платежа. Примечание: значение '{}' исключается из GL-проводки mp_acquiring.",
                EXCLUDED_PAYMENT_PROCESSING_VALUE
            ),
            field_id: "payment_processing".to_string(),
            value: extra_string_field(&item, "payment_processing")
                .unwrap_or_else(|| "-".to_string()),
        });

        let gl_entries = general_ledger_entries.get();
        let sort_field = sort_by.get();
        let is_desc = sort_desc.get();

        rows.sort_by(|a, b| {
            let cmp = match &*sort_field {
                "field_id" => a.field_id.cmp(&b.field_id),
                "gl_role" => gl_role_sort_label(field_gl_role(&a.field_id, &gl_entries))
                    .cmp(gl_role_sort_label(field_gl_role(&b.field_id, &gl_entries))),
                "financial_result" => {
                    financial_result_sort_order(field_financial_result_role(a, &gl_entries))
                        .cmp(&financial_result_sort_order(field_financial_result_role(
                            b,
                            &gl_entries,
                        )))
                        .then_with(|| a.description.cmp(&b.description))
                }
                "value" => a.value.cmp(&b.value),
                _ => a.description.cmp(&b.description),
            };
            if is_desc {
                cmp.reverse()
            } else {
                cmp
            }
        });

        rows
    };

    // Отфильтрованный список: по нему рисуется таблица и выгружается CSV,
    // иначе поиск и выгрузка разошлись бы. Второе значение — сколько строк всего.
    let visible_rows = move || -> (Vec<FieldView>, usize) {
        let entries = general_ledger_entries.get();
        let all: Vec<FieldView> = get_field_rows()
            .iter()
            .map(|row| FieldView::new(row, &entries))
            .collect();
        let total = all.len();

        let is_compact = compact.get();
        let selected = category.get();
        let needle = query.get().trim().to_lowercase();

        let rows = all
            .into_iter()
            .filter(|row| selected.accepts(row.gl_role))
            .filter(|row| needle.is_empty() || row.haystack(is_compact).contains(&needle))
            .collect();

        (rows, total)
    };

    // Ресайз колонок списка полей. Таблица появляется в DOM не сразу и пересоздаётся
    // при возврате на вкладку, поэтому эффект следит за вкладкой, а не отрабатывает
    // однократно; повторный вызов init_column_resize безопасен.
    Effect::new(move |_| {
        let is_fields = active_tab.get() == "fields";
        let has_data = data.get().is_some();
        if is_fields && has_data {
            spawn_local(async {
                gloo_timers::future::TimeoutFuture::new(50).await;
                init_column_resize(FIELDS_TABLE_ID, FIELDS_COLUMN_WIDTHS_KEY);
            });
        }
    });

    let handle_column_sort = move |column: &'static str| {
        let current_sort = sort_by.get();
        if current_sort == column {
            set_sort_desc.set(!sort_desc.get());
        } else {
            set_sort_by.set(column.to_string());
            set_sort_desc.set(false);
        }
    };

    // Экспорт в Excel — выгружается ровно то, что видно в списке после фильтров.
    let export_to_excel = move || {
        let (field_rows, _) = visible_rows();
        if field_rows.is_empty() {
            log!("No data to export");
            return;
        }

        // UTF-8 BOM для правильного отображения кириллицы в Excel
        let mut csv = String::from("\u{FEFF}");

        // Заголовок с точкой с запятой как разделитель
        csv.push_str("Описание;Роль;Результат;Идентификатор;Значение\n");

        for row in field_rows {
            csv.push_str(&format!(
                "\"{}\";\"{}\";\"{}\";\"{}\";\"{}\"\n",
                row.description.replace('\"', "\"\""),
                gl_role_sort_label(row.gl_role).replace('\"', "\"\""),
                financial_result_badge_label(row.result_role).replace('\"', "\"\""),
                row.field_id.replace('\"', "\"\""),
                row.value.replace('\"', "\"\"")
            ));
        }

        // Создаем Blob с CSV данными
        use js_sys::Array;
        use wasm_bindgen::JsValue;

        let array = Array::new();
        array.push(&JsValue::from_str(&csv));

        let blob_props = web_sys::BlobPropertyBag::new();
        blob_props.set_type("text/csv;charset=utf-8;");

        if let Ok(blob) = web_sys::Blob::new_with_str_sequence_and_options(&array, &blob_props) {
            if let Ok(url) = web_sys::Url::create_object_url_with_blob(&blob) {
                if let Some(window) = web_sys::window() {
                    if let Some(document) = window.document() {
                        if let Ok(a) = document.create_element("a") {
                            let a: web_sys::HtmlAnchorElement = a.unchecked_into();
                            a.set_href(&url);
                            let filename = format!(
                                "wb_finance_report_detail_{}.csv",
                                chrono::Utc::now().format("%Y%m%d_%H%M%S")
                            );
                            a.set_download(&filename);
                            let _ = a.click();
                            let _ = web_sys::Url::revoke_object_url(&url);
                        }
                    }
                }
            }
        }
    };

    let post_click = move |_| {
        let id = id.clone();
        set_posting.set(true);
        set_action_message.set(None);

        spawn_local(async move {
            match post_detail(&id).await {
                Ok(response) => {
                    set_general_ledger_entries.set(response.general_ledger_entries);
                    set_data.set(Some(response.item));
                    set_active_tab.set("general_ledger");
                    set_action_message.set(Some("General Ledger rebuilt.".to_string()));
                }
                Err(e) => {
                    log!("Failed to rebuild p903 general ledger: {:?}", e);
                    set_action_message.set(Some(format!("Post failed: {e}")));
                }
            }
            set_posting.set(false);
        });
    };

    view! {
        <PageFrame page_id="p903_wb_finance_report--detail" category="detail" class="p903-detail">
            <div class="page__header">
                <h3 class="page__title">"WB Finance Report Details"</h3>
                <div class="page__actions">
                    <Button
                        appearance=ButtonAppearance::Primary
                        size=ButtonSize::Small
                        on_click=post_click
                        disabled=Signal::derive(move || loading.get() || posting.get())
                    >
                        {icon("refresh")}
                        {move || if posting.get() { " Проведение..." } else { " Post" }}
                    </Button>
                    <Button
                        appearance=ButtonAppearance::Secondary
                        size=ButtonSize::Small
                        on_click=move |_| on_close.run(())
                    >
                        {icon("x")}
                        " Закрыть"
                    </Button>
                </div>
            </div>

            <div class="page__content">
            {move || {
                action_message.get().map(|message| {
                    view! {
                        <div class="warning-box" style="margin-bottom: var(--spacing-md);">
                            <span class="warning-box__text">{message}</span>
                        </div>
                    }
                })
            }}
            {move || {
                if loading.get() {
                    view! { <p class="text-muted">"Загрузка..."</p> }.into_any()
                } else if let Some(err) = error.get() {
                    view! {
                        <div class="warning-box warning-box--error">
                            <span class="warning-box__icon">"⚠"</span>
                            <span class="warning-box__text">{err}</span>
                        </div>
                    }
                        .into_any()
                } else if data.get().is_some() {
                    view! {
                        <div>
                            <div class="detail-tabs">
                                {["fields", "json", "links", "general_ledger"]
                                    .into_iter()
                                    .zip(["Fields", "Raw JSON", "Links", "General Ledger"])
                                    .map(|(key, label)| view! {
                                        <button
                                            class="detail-tabs__item"
                                            class:detail-tabs__item--active=move || active_tab.get() == key
                                            on:click=move |_| set_active_tab.set(key)
                                        >
                                            {label}
                                        </button>
                                    })
                                    .collect_view()}
                            </div>

                            // Tab Content
                            {move || {
                                if active_tab.get() == "fields" {
                                    let tabs_store = tabs_store.clone();
                                    let export_excel = export_to_excel.clone();
                                    view! {
                                        <div class="spec-list" class:spec-list--compact=move || compact.get()>
                                            <div class="spec-list__toolbar">
                                                <div class="spec-list__search">
                                                    <span class="spec-list__search-icon">{icon("search")}</span>
                                                    <input
                                                        class="form__input"
                                                        type="text"
                                                        placeholder="Поиск по видимым текстам"
                                                        prop:value=move || query.get()
                                                        on:input=move |ev| query.set(event_target_value(&ev))
                                                    />
                                                    <Show when=move || !query.get().is_empty()>
                                                        <button
                                                            class="spec-list__search-clear"
                                                            title="Очистить"
                                                            on:click=move |_| query.set(String::new())
                                                        >
                                                            {icon("x")}
                                                        </button>
                                                    </Show>
                                                </div>

                                                // Фильтр по категории строки (роль поля в проводках ГК)
                                                <div class="dpc-mode-tabs">
                                                    {CATEGORY_FILTERS
                                                        .into_iter()
                                                        .map(|item| view! {
                                                            <button
                                                                class="dpc-mode-tab"
                                                                class:dpc-mode-tab--active=move || category.get() == item
                                                                on:click=move |_| category.set(item)
                                                            >
                                                                {item.label()}
                                                            </button>
                                                        })
                                                        .collect_view()}
                                                </div>

                                                // Режим отображения: в кратком описания пунктов скрыты
                                                <div class="dpc-mode-tabs">
                                                    <button
                                                        class="dpc-mode-tab"
                                                        class:dpc-mode-tab--active=move || compact.get()
                                                        on:click=move |_| compact.set(true)
                                                    >
                                                        "Кратко"
                                                    </button>
                                                    <button
                                                        class="dpc-mode-tab"
                                                        class:dpc-mode-tab--active=move || !compact.get()
                                                        on:click=move |_| compact.set(false)
                                                    >
                                                        "Подробно"
                                                    </button>
                                                </div>

                                                <span class="spec-list__count">
                                                    {move || {
                                                        let (rows, total) = visible_rows();
                                                        format!("Показано {} из {}", rows.len(), total)
                                                    }}
                                                </span>

                                                <span class="spec-list__toolbar-spacer"></span>

                                                <Button
                                                    appearance=ButtonAppearance::Secondary
                                                    size=ButtonSize::Small
                                                    on_click=move |_| export_excel()
                                                >
                                                    {icon("download")}
                                                    " Excel (csv)"
                                                </Button>
                                            </div>

                                            <div class="table-wrapper">
                                                <table id=FIELDS_TABLE_ID class="table__data">
                                                    <thead class="table__head">
                                                        <tr>
                                                            {FIELD_COLUMNS
                                                                .into_iter()
                                                                .map(|(key, title, width)| view! {
                                                                    <th class="table__header-cell resizable" style=format!("width:{width};")>
                                                                        <div
                                                                            class="table__sortable-header"
                                                                            on:click=move |_| handle_column_sort(key)
                                                                        >
                                                                            {title}
                                                                            <span class=move || get_sort_class(&sort_by.get(), key)>
                                                                                {move || get_sort_indicator(&sort_by.get(), key, !sort_desc.get())}
                                                                            </span>
                                                                        </div>
                                                                    </th>
                                                                })
                                                                .collect_view()}
                                                        </tr>
                                                    </thead>
                                                    <tbody>
                                                        // Список перерисовывается целиком: строк ~40, а <For> по ключу
                                                        // не обновил бы роль и результат после проведения документа —
                                                        // ключи те же, а содержимое другое.
                                                        {move || {
                                                            let tabs_store = tabs_store.clone();
                                                            visible_rows().0.into_iter().map(move |row| {
                                                                let tabs_store = tabs_store.clone();
                                                                let value = row.value.clone();
                                                                let link_target = row.link_target();
                                                                view! {
                                                                    <tr
                                                                        class="spec-list__row"
                                                                        style=format!(
                                                                            "--spec-cat:{};",
                                                                            gl_role_cat_color(row.gl_role, row.derived),
                                                                        )
                                                                    >
                                                                        <td class="table__cell">
                                                                            <div class="spec-list__name">{row.description.clone()}</div>
                                                                            {row.note.map(|note| view! {
                                                                                <div class="spec-list__note">{note}</div>
                                                                            })}
                                                                        </td>
                                                                        <td class="table__cell">
                                                                            {match row.gl_role {
                                                                                Some(role) => view! {
                                                                                    <span class=gl_role_badge_class(role)>
                                                                                        {gl_role_badge_label(role)}
                                                                                    </span>
                                                                                }.into_any(),
                                                                                None => view! {
                                                                                    <span class="text-muted">"—"</span>
                                                                                }.into_any(),
                                                                            }}
                                                                        </td>
                                                                        <td class="table__cell">
                                                                            <span class=financial_result_badge_class(row.result_role)>
                                                                                {financial_result_badge_label(row.result_role)}
                                                                            </span>
                                                                        </td>
                                                                        <td class="table__cell">
                                                                            <span class="spec-list__ident">{row.field_id.clone()}</span>
                                                                        </td>
                                                                        <td class="table__cell">
                                                                            {if let Some((tab_key, tab_label)) = link_target {
                                                                                view! {
                                                                                    <a href="#" class="table__link" on:click=move |ev| {
                                                                                        ev.prevent_default();
                                                                                        tabs_store.open_tab(&tab_key, &tab_label);
                                                                                    }>{value}</a>
                                                                                }.into_any()
                                                                            } else if row.emphasized {
                                                                                view! { <code class="spec-list__code">{value}</code> }.into_any()
                                                                            } else {
                                                                                view! { <span class="spec-list__name">{value}</span> }.into_any()
                                                                            }}
                                                                        </td>
                                                                    </tr>
                                                                }
                                                            })
                                                            .collect_view()
                                                        }}
                                                    </tbody>
                                                </table>
                                            </div>

                                            <Show when=move || visible_rows().0.is_empty()>
                                                <div class="spec-list__empty">
                                                    "Ничего не найдено — измените запрос или снимите фильтр категории."
                                                </div>
                                            </Show>
                                        </div>
                                    }
                                        .into_any()
                                } else if active_tab.get() == "json" {
                                    let json_content = data
                                        .get()
                                        .and_then(|d| d.extra)
                                        .unwrap_or_else(|| "{}".to_string());
                                    view! {
                                        <JsonViewer
                                            json_content=json_content
                                            title="Raw JSON from WB".to_string()
                                        />
                                    }
                                        .into_any()
                                } else if active_tab.get() == "general_ledger" {
                                    let entries = Signal::derive(move || general_ledger_entries.get());
                                    view! {
                                        <DocumentGeneralLedgerEntries
                                            entries=entries
                                            loading=Signal::derive(|| false)
                                            error=Signal::derive(|| None::<String>)
                                            nav_id=document_general_ledger_entries_nav_id("p903_wb_finance_report")
                                            title="Журнал операций"
                                            empty_message="Нет связанных записей general ledger. Проведите документ для формирования проводок."
                                        />
                                    }
                                    .into_any()
                                } else if active_tab.get() == "links" {
                                    if links_loading.get() {
                                        view! { <p class="text-muted">"Загрузка связанных документов..."</p> }.into_any()
                                    } else if let Some(err) = links_error.get() {
                                        view! {
                                            <div class="warning-box warning-box--error">
                                                <span class="warning-box__icon">"⚠"</span>
                                                <span class="warning-box__text">"Error loading links: " {err}</span>
                                            </div>
                                        }
                                            .into_any()
                                    } else {
                                        let sales = linked_sales.get();
                                        if sales.is_empty() {
                                            view! { <p class="text-muted">"Нет связанных документов продаж для данного SRID."</p> }.into_any()
                                        } else {
                                            let total_qty: f64 = sales.iter().map(|s| s.line.qty).sum();
                                            let total_total_price: f64 = sales.iter().filter_map(|s| s.line.total_price).sum();
                                            let total_payment: f64 = sales.iter().filter_map(|s| s.line.payment_sale_amount).sum();
                                            let total_amount: f64 = sales.iter().filter_map(|s| s.line.amount_line).sum();
                                            let total_finished: f64 = sales.iter().filter_map(|s| s.line.finished_price).sum();

                                            view! {
                                                <div>
                                                    <div class="list-summary-bar">
                                                        <span>"Найдено: " {sales.len()} " документов"</span>
                                                        <span>"Total Qty: " {format_number(total_qty)}</span>
                                                        <span>"Total Price: " {format_number(total_total_price)}</span>
                                                        <span>"Payment: " {format_number(total_payment)}</span>
                                                        <span>"Amount: " {format_number(total_amount)}</span>
                                                        <span>"Finished: " {format_number(total_finished)}</span>
                                                    </div>

                                                    <div class="table-wrapper">
                                                        <Table attr:style="width:100%;table-layout:fixed;">
                                                            <TableHeader>
                                                                <TableRow>
                                                                    <TableHeaderCell attr:style="width:96px;">"Date"</TableHeaderCell>
                                                                    <TableHeaderCell attr:style="width:120px;">"Document No"</TableHeaderCell>
                                                                    <TableHeaderCell attr:style="width:84px;">"NM ID"</TableHeaderCell>
                                                                    <TableHeaderCell attr:style="width:120px;">"Supplier Article"</TableHeaderCell>
                                                                    <TableHeaderCell attr:style="width:auto;">"Name"</TableHeaderCell>
                                                                    <TableHeaderCell attr:style="width:64px;text-align:right;">"Qty"</TableHeaderCell>
                                                                    <TableHeaderCell attr:style="width:96px;text-align:right;">"Total Price"</TableHeaderCell>
                                                                    <TableHeaderCell attr:style="width:96px;text-align:right;">"Payment"</TableHeaderCell>
                                                                    <TableHeaderCell attr:style="width:96px;text-align:right;">"Price Eff."</TableHeaderCell>
                                                                    <TableHeaderCell attr:style="width:96px;text-align:right;">"Amount Line"</TableHeaderCell>
                                                                    <TableHeaderCell attr:style="width:96px;text-align:right;">"Finished Price"</TableHeaderCell>
                                                                </TableRow>
                                                            </TableHeader>
                                                            <TableBody>
                                                                {sales
                                                                    .into_iter()
                                                                    .map(|sale| {
                                                                        let sale_id = sale.id.clone();
                                                                        view! {
                                                                            <TableRow attr:style="cursor:pointer;" on:click=move |_| set_selected_sale_id.set(Some(sale_id.clone()))>
                                                                                <TableCell attr:style="width:96px;"><TableCellLayout truncate=true>{sale.state.sale_dt}</TableCellLayout></TableCell>
                                                                                <TableCell attr:style="width:120px;"><TableCellLayout truncate=true>{sale.header.document_no}</TableCellLayout></TableCell>
                                                                                <TableCell attr:style="width:84px;"><TableCellLayout truncate=true>{sale.line.nm_id}</TableCellLayout></TableCell>
                                                                                <TableCell attr:style="width:120px;"><TableCellLayout truncate=true>{sale.line.supplier_article}</TableCellLayout></TableCell>
                                                                                <TableCell><TableCellLayout truncate=true>{sale.line.name}</TableCellLayout></TableCell>
                                                                                <TableCell attr:style="width:64px;text-align:right;"><TableCellLayout attr:style="display:block;width:100%;text-align:right;">{format_number(sale.line.qty)}</TableCellLayout></TableCell>
                                                                                <TableCell attr:style="width:96px;text-align:right;"><TableCellLayout attr:style="display:block;width:100%;text-align:right;">{sale.line.total_price.map(|v| format_number(v)).unwrap_or_else(|| "-".to_string())}</TableCellLayout></TableCell>
                                                                                <TableCell attr:style="width:96px;text-align:right;"><TableCellLayout attr:style="display:block;width:100%;text-align:right;">{sale.line.payment_sale_amount.map(|v| format_number(v)).unwrap_or_else(|| "-".to_string())}</TableCellLayout></TableCell>
                                                                                <TableCell attr:style="width:96px;text-align:right;"><TableCellLayout attr:style="display:block;width:100%;text-align:right;">{sale.line.price_effective.map(|v| format_number(v)).unwrap_or_else(|| "-".to_string())}</TableCellLayout></TableCell>
                                                                                <TableCell attr:style="width:96px;text-align:right;"><TableCellLayout attr:style="display:block;width:100%;text-align:right;">{sale.line.amount_line.map(|v| format_number(v)).unwrap_or_else(|| "-".to_string())}</TableCellLayout></TableCell>
                                                                                <TableCell attr:style="width:96px;text-align:right;"><TableCellLayout attr:style="display:block;width:100%;text-align:right;">{sale.line.finished_price.map(|v| format_number(v)).unwrap_or_else(|| "-".to_string())}</TableCellLayout></TableCell>
                                                                            </TableRow>
                                                                        }
                                                                        .into_view()
                                                                    })
                                                                    .collect_view()}
                                                            </TableBody>
                                                        </Table>
                                                    </div>
                                                </div>
                                            }.into_any()
                                        }
                                    }
                                } else {
                                    view! { <></> }.into_any()
                                }
                            }}

                        </div>
                    }
                        .into_any()
                } else {
                    view! { <p>"Нет данных"</p> }.into_any()
                }
            }}
            </div>

            // Modal for WbSalesDetail when clicking on a linked sale
            {move || {
                if let Some(sale_id) = selected_sale_id.get() {
                    view! {
                        <div class="modal-overlay" style="z-index: 2000;">
                            <div class="modal modal-content-wide">
                                <WbSalesDetail
                                    id=sale_id.clone()
                                    on_close=move || set_selected_sale_id.set(None)
                                />
                            </div>
                        </div>
                    }.into_any()
                } else {
                    view! { <></> }.into_any()
                }
            }}
        </PageFrame>
    }
}
