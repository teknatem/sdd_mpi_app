use crate::domain::common::{
    AggregateId, AggregateRoot, BaseAggregate, EntityMetadata, Origin,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ─────────────────────────────────────────────────────────────────────────────
// ID
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WbDayCloseId(pub Uuid);

impl WbDayCloseId {
    pub fn new(value: Uuid) -> Self {
        Self(value)
    }
    pub fn new_v4() -> Self {
        Self(Uuid::new_v4())
    }
    pub fn value(&self) -> Uuid {
        self.0
    }
}

impl AggregateId for WbDayCloseId {
    fn as_string(&self) -> String {
        self.0.to_string()
    }
    fn from_string(s: &str) -> Result<Self, String> {
        Uuid::parse_str(s)
            .map(WbDayCloseId::new)
            .map_err(|e| format!("Invalid UUID: {}", e))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Supporting enums
// ─────────────────────────────────────────────────────────────────────────────

/// Тип события строки дня (legacy — используется для обратной совместимости).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SaleEvent {
    Sale,
    Return,
    Mixed,
}

impl SaleEvent {
    pub fn label(&self) -> &'static str {
        match self {
            SaleEvent::Sale => "Продажа",
            SaleEvent::Return => "Возврат",
            SaleEvent::Mixed => "Продажа+Возврат",
        }
    }
}

impl Default for SaleEvent {
    fn default() -> Self {
        SaleEvent::Sale
    }
}

/// Классификатор типа строки документа a033, синхронизирован с GL-логикой p903.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LineKind {
    /// Продажа: srid есть, supplier_oper_name='Продажа' или retail_amount > 0.
    Sale,
    /// Возврат: srid есть, supplier_oper_name='Возврат' или return_amount > 0.
    Return,
    /// Корректировка комиссии (не продажа/возврат, есть ppvz-суммы).
    CommissionAdjustment,
    /// Логистика: rebill_logistic_cost.
    Logistics,
    /// Хранение: storage_fee (обычно без srid).
    Storage,
    /// Штраф: penalty (обычно без srid).
    Penalty,
    /// Возмещение за выдачу/возврат товаров на ПВЗ.
    PpvzReward,
    /// Добровольная компенсация при возврате.
    VoluntaryReturnCompensation,
    /// Возмещение издержек по перевозке/складским операциям.
    TransportStorageReimbursement,
    /// Приёмка (acceptance): delivery_amount, без srid.
    Acceptance,
    /// Прочее — не удалось классифицировать.
    Other,
    /// Информационная строка — все финансовые колонки 1-7 равны нулю.
    Info,
}

impl LineKind {
    pub fn label(&self) -> &'static str {
        match self {
            LineKind::Sale => "Продажа",
            LineKind::Return => "Возврат",
            LineKind::CommissionAdjustment => "Корр.комиссии",
            LineKind::Logistics => "Логистика",
            LineKind::Storage => "Хранение",
            LineKind::Penalty => "Штраф",
            LineKind::PpvzReward => "Возм.ПВЗ",
            LineKind::VoluntaryReturnCompensation => "Добр.компенс.",
            LineKind::TransportStorageReimbursement => "Возм.перевозки",
            LineKind::Acceptance => "Приёмка",
            LineKind::Other => "Прочее",
            LineKind::Info => "Инфо",
        }
    }

    /// Требуется ли связь с a015 (заказ).
    pub fn requires_order(&self) -> bool {
        matches!(self, LineKind::Sale | LineKind::Return)
    }

    /// Требуется ли связь с a012 (реализация).
    pub fn requires_sales_doc(&self) -> bool {
        matches!(self, LineKind::Sale | LineKind::Return)
    }

    /// Строки без финансового смысла — не показывать ЦенуДилера и маржу.
    pub fn is_info(&self) -> bool {
        matches!(self, LineKind::Info)
    }
}

impl Default for LineKind {
    fn default() -> Self {
        LineKind::Other
    }
}

/// Уровень детализации строки (по наличию srid и nm_id).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LineDetail {
    /// Есть srid и nm_id/nomenclature_ref.
    OrderAndNomenclature,
    /// Только srid (нет номенклатуры).
    OrderOnly,
    /// Только nm_id (нет srid).
    NomenclatureOnly,
    /// Нет ни srid, ни nm_id (общие удержания/хранение/штрафы).
    General,
}

impl LineDetail {
    pub fn label(&self) -> &'static str {
        match self {
            LineDetail::OrderAndNomenclature => "Заказ+Номенкл.",
            LineDetail::OrderOnly => "Только заказ",
            LineDetail::NomenclatureOnly => "Только номенкл.",
            LineDetail::General => "Общая",
        }
    }
}

impl Default for LineDetail {
    fn default() -> Self {
        LineDetail::General
    }
}

/// Серьёзность проблемы.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ProblemSeverity {
    Info,
    Warn,
    Block,
}

impl ProblemSeverity {
    pub fn label(&self) -> &'static str {
        match self {
            ProblemSeverity::Info => "Информация",
            ProblemSeverity::Warn => "Предупреждение",
            ProblemSeverity::Block => "Блокировка",
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Line (строка таблицы — 10 колонок)
// ─────────────────────────────────────────────────────────────────────────────

/// Одна строка документа: агрегат по (srid, nomenclature_ref).
/// Знаковое соглашение: доход +, расход −.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct WbDayCloseLine {
    /// Идентификатор заказа из WB (поле srid в p903). Пустой для «общих» строк.
    pub srid: String,

    /// Ссылка на номенклатуру (a004_nomenclature.id), если разрезолвлена.
    pub nomenclature_ref: Option<String>,

    /// WB nm_id (числовой артикул WB).
    pub nm_id: Option<i64>,

    /// Артикул продавца (sa_name из p903).
    pub sa_name: Option<String>,

    /// Тип события строки (legacy, для обратной совместимости).
    pub event: SaleEvent,

    /// Классификатор типа строки (новый, точный).
    #[serde(default)]
    pub kind: LineKind,

    /// Уровень детализации строки.
    #[serde(default)]
    pub detail: LineDetail,

    /// Количество продано.
    pub qty_sold: i64,

    /// Количество возвращено.
    pub qty_returned: i64,

    // ── Связь с a015_wb_orders ───────────────────────────────────────────────
    /// UUID документа a015 (заказ WB), если найден.
    #[serde(default)]
    pub order_id: Option<String>,

    /// Дата заказа (YYYY-MM-DD) из a015.state.order_dt.
    #[serde(default)]
    pub order_date: Option<String>,

    /// true, если заказ помечен отменённым в a015.
    #[serde(default)]
    pub order_is_cancelled: bool,

    // ── Связь с a012_wb_sales ────────────────────────────────────────────────
    /// UUID документа a012 (реализация/возврат), соответствующего типу строки.
    #[serde(default)]
    pub sales_doc_id: Option<String>,

    /// document_no из a012 (= srid WB).
    #[serde(default)]
    pub sales_doc_no: Option<String>,

    /// sale_date из a012 (YYYY-MM-DD, первые 10 символов).
    #[serde(default)]
    pub sales_doc_date: Option<String>,

    /// event_type из a012 («sale» или «return»).
    #[serde(default)]
    pub sales_event_type: Option<String>,

    /// UUID лишних a012 для этого srid (если > 1 — проблема).
    #[serde(default)]
    pub sales_extra_ids: Vec<String>,

    /// sale_id из a012_wb_sales (WB-идентификатор продажи/возврата).
    #[serde(default)]
    pub sales_sale_id: Option<String>,

    // ── Ссылка на исходный p903 ──────────────────────────────────────────────
    /// UUID первой строки p903_wb_finance_report этой группы.
    #[serde(default)]
    pub p903_ref_id: Option<String>,

    /// rrd_id из p903 (числовой WB-идентификатор строки финансового отчёта).
    #[serde(default)]
    pub p903_rrd_id: Option<i64>,

    // ── 10 колонок ───────────────────────────────────────────────────────────
    /// 1. Реализация.
    ///    Продажа: retail_amount − return_amount (> 0).
    ///    Возврат: −retail_amount или −return_amount (< 0, WB кладёт сумму в retail_amount).
    pub revenue: f64,

    /// 2. Реклама: −SUM(p913.amount WHERE turnover_code='advert_clicks_order_expense' AND order_key=srid)
    pub advertising: f64,

    /// 3. Логистика: −(delivery_rub + rebill_logistic_cost + storage_fee)
    pub logistics: f64,

    /// 4. Эквайринг.
    ///    Продажа: −acquiring_fee (расход).
    ///    Возврат: +acquiring_fee (WB возвращает комиссию эквайрера → доход/сторно).
    pub acquiring: f64,

    /// 5. Комиссия.
    ///    Sale/Return: ±(ppvz_vw + ppvz_vw_nds) — ppvz_sales_commission исключён (GL-rule).
    ///    Прочие:      −(ppvz_vw + ppvz_vw_nds + ppvz_sales_commission).
    ///      • ppvz > 0 → WB возвращает комиссию → положительное (доход).
    ///      • ppvz < 0 → WB берёт обратно соинвест → отрицательное (расход).
    pub commission: f64,

    /// 6. Штрафы: −SUM(penalty)
    pub penalty: f64,

    /// 7. Прочее: SUM(additional_payment) + SUM(cashback_amount)
    pub other: f64,

    /// 8. Результат: Σ колонок 1..7 (вычисляемое).
    pub result: f64,

    /// 9. ЦенаДилер: −(dealer_price_ut × qty) из a012, fallback p912.
    pub dealer_price: f64,

    /// 10. Сравнение: Результат + ЦенаДилер (прибыль/маржа по строке).
    pub margin_diff: f64,

    /// Коды проблем, относящихся к этой строке.
    #[serde(default)]
    pub problem_codes: Vec<String>,
}

impl WbDayCloseLine {
    /// Проверяет инвариант: columns 1..7 == result.
    pub fn check_invariant(&self) -> bool {
        let expected = self.revenue
            + self.advertising
            + self.logistics
            + self.acquiring
            + self.commission
            + self.penalty
            + self.other;
        (expected - self.result).abs() < 0.001
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Advert snapshot lines
// ─────────────────────────────────────────────────────────────────────────────

/// Строка рекламного снапшота из p911 (advert_clicks_no_order).
/// Хранится в документе как JSON-массив, заполняется при recalculate.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct WbDayCloseAdvertNoOrderLine {
    /// p911_wb_advert_by_items.id
    pub projection_ref_id: String,
    pub nomenclature_ref: Option<String>,
    pub sa_name: Option<String>,
    pub amount: f64,
    pub general_ledger_ref: Option<String>,
    /// wb_advert_campaign_code из p911 (числовой advert_id WB, для отображения)
    pub campaign_code: String,
    /// UUID записи a030_wb_advert_campaign (для ссылки), если найден
    pub campaign_ref: Option<String>,
}

/// Строка рекламного снапшота из p913 (advert_clicks_order_accrual).
/// Хранится в документе как JSON-массив, заполняется при recalculate.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct WbDayCloseAdvertOrderAccrualLine {
    /// p913_wb_advert_order_attr.id
    pub projection_ref_id: String,
    pub nomenclature_ref: Option<String>,
    pub sa_name: Option<String>,
    pub amount: f64,
    /// order_key из p913 (= srid из p903)
    pub order_key: String,
    /// UUID документа a015 (если найден)
    pub order_id: Option<String>,
    pub order_date: Option<String>,
    /// wb_advert_campaign_code из p913 (числовой advert_id WB, для отображения)
    pub campaign_code: String,
    /// UUID записи a030_wb_advert_campaign (для ссылки), если найден
    pub campaign_ref: Option<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Problem
// ─────────────────────────────────────────────────────────────────────────────

/// Проблема, обнаруженная детектором.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WbDayCloseProblem {
    /// Код детектора, например "advert_clicks_order_accrual_without_expense".
    pub code: String,

    /// Серьёзность.
    pub severity: ProblemSeverity,

    /// srid, к которому относится проблема (если есть).
    pub srid: Option<String>,

    /// Ссылка на номенклатуру (если есть).
    pub nomenclature_ref: Option<String>,

    /// UUID документов a012_wb_sales, которые нужно перепровести для устранения проблемы.
    #[serde(default)]
    pub a012_ids: Vec<String>,

    /// Сумма advert_clicks_order_expense по связанным a012 (если есть).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub a012_advert_expense: Option<f64>,

    /// Человекочитаемое описание проблемы.
    pub message: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Totals
// ─────────────────────────────────────────────────────────────────────────────

/// Итоговые суммы по документу (сумма всех строк).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct WbDayCloseTotals {
    pub lines_count: i64,
    pub revenue: f64,
    pub advertising: f64,
    pub logistics: f64,
    pub acquiring: f64,
    pub commission: f64,
    pub penalty: f64,
    pub other: f64,
    pub result: f64,
    pub dealer_price: f64,
    pub margin_diff: f64,
    pub problems_block: i64,
    pub problems_warn: i64,
    pub problems_info: i64,
    /// Число строк, у которых есть хотя бы одна проблема.
    #[serde(default)]
    pub problem_lines: i64,
}

impl WbDayCloseTotals {
    pub fn from_lines(lines: &[WbDayCloseLine], problems: &[WbDayCloseProblem]) -> Self {
        let mut t = WbDayCloseTotals::default();
        t.lines_count = lines.len() as i64;
        for line in lines {
            t.revenue += line.revenue;
            t.advertising += line.advertising;
            t.logistics += line.logistics;
            t.acquiring += line.acquiring;
            t.commission += line.commission;
            t.penalty += line.penalty;
            t.other += line.other;
            t.result += line.result;
            t.dealer_price += line.dealer_price;
            t.margin_diff += line.margin_diff;
            if !line.problem_codes.is_empty() && !line.kind.is_info() {
                t.problem_lines += 1;
            }
        }
        for p in problems {
            match p.severity {
                ProblemSeverity::Block => t.problems_block += 1,
                ProblemSeverity::Warn => t.problems_warn += 1,
                ProblemSeverity::Info => t.problems_info += 1,
            }
        }
        t
    }

    pub fn total_problems(&self) -> i64 {
        self.problems_block + self.problems_warn + self.problems_info
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Root aggregate
// ─────────────────────────────────────────────────────────────────────────────

/// Закрытие дня WB-кабинета.
///
/// Один активный документ на (connection_id, business_date),
/// произвольное количество архивных для сравнения.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WbDayClose {
    #[serde(flatten)]
    pub base: BaseAggregate<WbDayCloseId>,

    /// ID подключения МП (a006_connection_mp.id).
    pub connection_id: String,

    /// Дата дня в формате YYYY-MM-DD.
    pub business_date: String,

    /// true — архивная копия; false — активная.
    pub is_archived: bool,

    /// Когда был заархивирован.
    pub archived_at: Option<String>,

    /// Причина архивации.
    pub archived_reason: Option<String>,

    /// ID документа, который этот заменил (при archive_and_recreate).
    pub replaces_id: Option<String>,

    /// Время последнего пересчёта строк.
    pub last_recalculated_at: Option<String>,

    /// SHA-256 канонического JSON (lines + problems + totals) для сравнения снапшотов.
    pub snapshot_hash: String,

    /// Строки документа.
    #[serde(default)]
    pub lines: Vec<WbDayCloseLine>,

    /// Список проблем, обнаруженных детекторами.
    #[serde(default)]
    pub problems: Vec<WbDayCloseProblem>,

    /// Итоги по документу.
    pub totals: WbDayCloseTotals,

    /// Снапшот рекламных строк из p911 (advert_clicks_no_order).
    #[serde(default)]
    pub advert_clicks_no_order_lines: Vec<WbDayCloseAdvertNoOrderLine>,

    /// Снапшот рекламных строк из p913 (advert_clicks_order_accrual).
    #[serde(default)]
    pub advert_clicks_order_accrual_lines: Vec<WbDayCloseAdvertOrderAccrualLine>,

    /// GL-итог из sys_general_ledger по advert_clicks_no_order, заполняется при recalculate.
    #[serde(default)]
    pub gl_advert_no_order: f64,

    /// GL-итог из sys_general_ledger по advert_clicks_order_accrual, заполняется при recalculate.
    #[serde(default)]
    pub gl_advert_order_accrual: f64,

    /// GL-итог из sys_general_ledger по advert_clicks_order_expense, заполняется при recalculate.
    #[serde(default)]
    pub gl_advert_order_expense: f64,

    /// Снапшот p913 (advert_clicks_order_expense) за дату документа — для колонки «Документ».
    #[serde(default)]
    pub snap_advert_order_expense: f64,
}

impl WbDayClose {
    pub fn new_active(connection_id: String, business_date: String) -> Self {
        let code = format!(
            "WDC-{}-{}",
            &business_date,
            &connection_id[..8.min(connection_id.len())]
        );
        let description = format!(
            "Закрытие дня WB {} за {}",
            &connection_id[..8.min(connection_id.len())],
            business_date
        );
        let base = BaseAggregate::new(WbDayCloseId::new_v4(), code, description);

        Self {
            base,
            connection_id,
            business_date,
            is_archived: false,
            archived_at: None,
            archived_reason: None,
            replaces_id: None,
            last_recalculated_at: None,
            snapshot_hash: String::new(),
            lines: Vec::new(),
            problems: Vec::new(),
            totals: WbDayCloseTotals::default(),
            advert_clicks_no_order_lines: Vec::new(),
            advert_clicks_order_accrual_lines: Vec::new(),
            gl_advert_no_order: 0.0,
            gl_advert_order_accrual: 0.0,
            gl_advert_order_expense: 0.0,
            snap_advert_order_expense: 0.0,
        }
    }

    pub fn touch_updated(&mut self) {
        self.base.touch();
    }

    pub fn before_write(&mut self) {
        self.touch_updated();
    }

    pub fn mark_archived(&mut self, reason: Option<String>) {
        self.is_archived = true;
        self.archived_at = Some(Utc::now().to_rfc3339());
        self.archived_reason = reason;
        self.before_write();
    }

    pub fn set_lines_and_problems(
        &mut self,
        lines: Vec<WbDayCloseLine>,
        problems: Vec<WbDayCloseProblem>,
    ) {
        self.totals = WbDayCloseTotals::from_lines(&lines, &problems);
        self.lines = lines;
        self.problems = problems;
        self.last_recalculated_at = Some(Utc::now().to_rfc3339());
        self.recompute_snapshot_hash();
    }

    pub fn set_advert_lines(
        &mut self,
        no_order: Vec<WbDayCloseAdvertNoOrderLine>,
        order_accrual: Vec<WbDayCloseAdvertOrderAccrualLine>,
        gl_no_order: f64,
        gl_order_accrual: f64,
        gl_order_expense: f64,
        snap_order_expense: f64,
    ) {
        self.advert_clicks_no_order_lines = no_order;
        self.advert_clicks_order_accrual_lines = order_accrual;
        self.gl_advert_no_order = gl_no_order;
        self.gl_advert_order_accrual = gl_order_accrual;
        self.gl_advert_order_expense = gl_order_expense;
        self.snap_advert_order_expense = snap_order_expense;
        self.recompute_snapshot_hash();
    }

    /// Пересчитывает SHA-256 хэш снапшота по lines + problems + totals + advert snapshots.
    pub fn recompute_snapshot_hash(&mut self) {
        let snap = SnapshotData {
            lines: &self.lines,
            problems: &self.problems,
            totals: &self.totals,
            advert_no_order: &self.advert_clicks_no_order_lines,
            advert_order_accrual: &self.advert_clicks_order_accrual_lines,
        };

        if let Ok(json) = serde_json::to_string(&snap) {
            let hash = simple_hash(json.as_bytes());
            self.snapshot_hash = format!("{:016x}", hash);
        }
    }

    pub fn to_list_dto(&self) -> WbDayCloseListDto {
        WbDayCloseListDto {
            id: self.base.id.as_string(),
            connection_id: self.connection_id.clone(),
            business_date: self.business_date.clone(),
            is_archived: self.is_archived,
            archived_at: self.archived_at.clone(),
            archived_reason: self.archived_reason.clone(),
            last_recalculated_at: self.last_recalculated_at.clone(),
            snapshot_hash: self.snapshot_hash.clone(),
            lines_count: self.totals.lines_count,
            problems_block: self.totals.problems_block,
            problems_warn: self.totals.problems_warn,
            problems_info: self.totals.problems_info,
            problem_lines: self.totals.problem_lines,
            result: self.totals.result,
            margin_diff: self.totals.margin_diff,
            description: self.base.description.clone(),
            created_at: self.base.metadata.created_at.to_rfc3339(),
            updated_at: self.base.metadata.updated_at.to_rfc3339(),
        }
    }
}

/// Вспомогательная структура для хэширования.
#[derive(Serialize)]
struct SnapshotData<'a> {
    lines: &'a Vec<WbDayCloseLine>,
    problems: &'a Vec<WbDayCloseProblem>,
    totals: &'a WbDayCloseTotals,
    advert_no_order: &'a Vec<WbDayCloseAdvertNoOrderLine>,
    advert_order_accrual: &'a Vec<WbDayCloseAdvertOrderAccrualLine>,
}

/// Простая 64-bit хэш-функция (FNV-1a).
fn simple_hash(data: &[u8]) -> u64 {
    let mut hash: u64 = 14695981039346656037;
    for &b in data {
        hash ^= b as u64;
        hash = hash.wrapping_mul(1099511628211);
    }
    hash
}

// ─────────────────────────────────────────────────────────────────────────────
// DTOs
// ─────────────────────────────────────────────────────────────────────────────

/// DTO для списка документов.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WbDayCloseListDto {
    pub id: String,
    pub connection_id: String,
    pub business_date: String,
    pub is_archived: bool,
    pub archived_at: Option<String>,
    pub archived_reason: Option<String>,
    pub last_recalculated_at: Option<String>,
    pub snapshot_hash: String,
    pub lines_count: i64,
    pub problems_block: i64,
    pub problems_warn: i64,
    pub problems_info: i64,
    #[serde(default)]
    pub problem_lines: i64,
    pub result: f64,
    pub margin_diff: f64,
    pub description: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Запрос на создание активного документа.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateActiveRequest {
    pub connection_id: String,
    pub business_date: String,
}

/// Запрос на репост проблемных a012.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RepostProblematicRequest {
    /// Если задан — репостим только проблемы с этими кодами.
    #[serde(default)]
    pub only_problem_codes: Vec<String>,
}

/// Запрос на архивацию с созданием нового.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveAndRecreateRequest {
    pub reason: Option<String>,
}

/// Запрос на сравнение двух версий.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompareRequest {
    pub active_id: String,
    pub archived_id: String,
}

/// Ответ сравнения двух версий.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompareResponse {
    pub active_date: String,
    pub archived_date: Option<String>,
    pub active_totals: WbDayCloseTotals,
    pub archived_totals: WbDayCloseTotals,
    /// Строки, которые есть в активном, но не в архивном (по srid).
    pub added_srids: Vec<String>,
    /// Строки, которые были в архивном, но не в активном.
    pub removed_srids: Vec<String>,
    /// Строки с изменившимся result.
    pub changed_srids: Vec<SridDiff>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SridDiff {
    pub srid: String,
    pub active_result: f64,
    pub archived_result: f64,
    pub delta: f64,
}

/// Результат операции с прогрессом (для repost).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepostResult {
    pub total: usize,
    pub success: usize,
    pub failed: usize,
    pub errors: Vec<String>,
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_line(
        srid: &str,
        revenue: f64,
        advertising: f64,
        logistics: f64,
        acquiring: f64,
        commission: f64,
        penalty: f64,
        other: f64,
    ) -> WbDayCloseLine {
        let result = revenue + advertising + logistics + acquiring + commission + penalty + other;
        WbDayCloseLine {
            srid: srid.to_string(),
            revenue,
            advertising,
            logistics,
            acquiring,
            commission,
            penalty,
            other,
            result,
            dealer_price: 0.0,
            margin_diff: result,
            ..Default::default()
        }
    }

    #[test]
    fn check_invariant_holds_when_sum_correct() {
        let line = make_line("S001", 1000.0, -50.0, -65.0, -20.0, -115.0, 0.0, 0.0);
        assert!(
            line.check_invariant(),
            "invariant must hold for correctly computed line"
        );
    }

    #[test]
    fn check_invariant_fails_when_result_is_wrong() {
        let mut line = make_line("S002", 1000.0, -50.0, -65.0, -20.0, -115.0, 0.0, 0.0);
        line.result += 10.0; // Deliberately break the invariant
        assert!(
            !line.check_invariant(),
            "invariant must fail when result is wrong"
        );
    }

    #[test]
    fn snapshot_hash_is_stable_on_repeated_calls() {
        let mut doc = WbDayClose::new_active("conn-1".to_string(), "2026-01-15".to_string());
        let line = make_line("S001", 1000.0, -50.0, -65.0, -20.0, -115.0, 0.0, 0.0);
        let problems = vec![WbDayCloseProblem {
            code: "dealer_price_missing".to_string(),
            severity: ProblemSeverity::Warn,
            srid: Some("S001".to_string()),
            nomenclature_ref: None,
            a012_ids: vec![],
            a012_advert_expense: None,
            message: "test".to_string(),
        }];
        doc.lines = vec![line];
        doc.problems = problems;
        doc.totals = WbDayCloseTotals::from_lines(&doc.lines, &doc.problems);

        doc.recompute_snapshot_hash();
        let hash1 = doc.snapshot_hash.clone();

        doc.recompute_snapshot_hash();
        let hash2 = doc.snapshot_hash.clone();

        assert_eq!(
            hash1, hash2,
            "snapshot_hash must be stable on repeated calls"
        );
        assert!(!hash1.is_empty(), "snapshot_hash must not be empty");
    }

    #[test]
    fn snapshot_hash_changes_when_lines_change() {
        let mut doc = WbDayClose::new_active("conn-1".to_string(), "2026-01-15".to_string());
        doc.lines = vec![make_line("S001", 1000.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0)];
        doc.totals = WbDayCloseTotals::from_lines(&doc.lines, &doc.problems);
        doc.recompute_snapshot_hash();
        let hash1 = doc.snapshot_hash.clone();

        doc.lines
            .push(make_line("S002", 500.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0));
        doc.totals = WbDayCloseTotals::from_lines(&doc.lines, &doc.problems);
        doc.recompute_snapshot_hash();
        let hash2 = doc.snapshot_hash.clone();

        assert_ne!(hash1, hash2, "snapshot_hash must change when lines change");
    }

    #[test]
    fn totals_aggregation_is_correct() {
        let lines = vec![
            make_line("S001", 1000.0, -50.0, -65.0, -20.0, -115.0, 0.0, 0.0),
            make_line("S002", 800.0, 0.0, -30.0, -10.0, -80.0, -5.0, 0.0),
        ];
        let problems = vec![WbDayCloseProblem {
            code: "dealer_price_missing".to_string(),
            severity: ProblemSeverity::Warn,
            srid: None,
            nomenclature_ref: None,
            a012_ids: vec![],
            a012_advert_expense: None,
            message: String::new(),
        }];
        let totals = WbDayCloseTotals::from_lines(&lines, &problems);

        assert_eq!(totals.lines_count, 2);
        assert_eq!(totals.revenue, 1800.0);
        assert_eq!(totals.problems_warn, 1);
        assert_eq!(totals.problems_block, 0);
        assert_eq!(totals.total_problems(), 1);
    }

    #[test]
    fn new_active_creates_non_archived_document() {
        let doc = WbDayClose::new_active("conn-abc".to_string(), "2026-05-14".to_string());
        assert!(!doc.is_archived);
        assert!(doc.archived_at.is_none());
        assert!(doc.lines.is_empty());
        assert!(doc.problems.is_empty());
    }

    #[test]
    fn mark_archived_sets_all_fields() {
        let mut doc = WbDayClose::new_active("conn-abc".to_string(), "2026-05-14".to_string());
        doc.mark_archived(Some("test reason".to_string()));
        assert!(doc.is_archived);
        assert!(doc.archived_at.is_some());
        assert_eq!(doc.archived_reason.as_deref(), Some("test reason"));
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// AggregateRoot impl
// ─────────────────────────────────────────────────────────────────────────────

impl AggregateRoot for WbDayClose {
    type Id = WbDayCloseId;

    fn id(&self) -> Self::Id {
        self.base.id
    }
    fn code(&self) -> &str {
        &self.base.code
    }
    fn description(&self) -> &str {
        &self.base.description
    }
    fn metadata(&self) -> &EntityMetadata {
        &self.base.metadata
    }
    fn metadata_mut(&mut self) -> &mut EntityMetadata {
        &mut self.base.metadata
    }
    fn aggregate_index() -> &'static str {
        "a033"
    }
    fn collection_name() -> &'static str {
        "wb_day_close"
    }
    fn element_name() -> &'static str {
        "Закрытие дня WB"
    }
    fn list_name() -> &'static str {
        "Закрытие дня WB"
    }
    fn origin() -> Origin {
        Origin::Self_
    }
}
