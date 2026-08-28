//! Построение строк p916 (движений воронки) из регистраторов-источников.
//!
//! Каждый источник маппит свои данные в `Vec<Model>`. `id` строки — детерминированный
//! `uuid v5` от натурального ключа (см. `deterministic_id`), поэтому повторный прогон даёт
//! те же `id` и вставка через `on_conflict(id)` перезаписывает, а не задваивает. Вызывающая
//! сторона дополнительно делает delete-by-registrator/period (убрать исчезнувшие строки).
//! Пустые строки не пишутся (разреженность — контроль размера проекции).
//!
//! Стадия 1 (marketing) — из a036: cohort_date = event_date = день воронки.
//! Стадия 2 (fulfillment) — из a015/a012: cohort_date = дата заказа (когорта),
//! event_date = дата транзакции события. Отменённый заказ порождает ДВЕ строки:
//! «заказ» (на дату заказа) и «отмена» (на дату отмены), обе — один регистратор.
//!
//! Все метрики — ПОЛОЖИТЕЛЬНЫЕ величины, включая возвраты и отмены: знак несёт не
//! число, а вид движения. Потребитель вычитает возвраты из выкупов явно.

use chrono::{DateTime, FixedOffset, Utc};
use uuid::Uuid;

use contracts::domain::a012_wb_sales::aggregate::WbSales;
use contracts::domain::a013_ym_order::aggregate::YmOrder;
use contracts::domain::a015_wb_orders::aggregate::WbOrders;
use contracts::domain::a016_ym_returns::aggregate::YmReturn;
use contracts::domain::a026_wb_advert_daily::aggregate::WbAdvertDaily;
use contracts::domain::a036_wb_sales_funnel_daily::aggregate::WbSalesFunnelDaily;
use contracts::projections::p916_mp_sales_funnel_turnovers::dto::FunnelStage;

use super::repository::Model;

pub const REG_A036: &str = "a036_wb_sales_funnel_daily";
pub const REG_A015: &str = "a015_wb_orders";
pub const REG_A012: &str = "a012_wb_sales";
pub const REG_A026: &str = "a026_wb_advert_daily";
pub const REG_A013: &str = "a013_ym_order";
pub const REG_A016: &str = "a016_ym_returns";
pub const REG_A041: &str = "a041_ym_shows_sales_daily";

/// Namespace для детерминированного `id` строки движения (uuid v5).
/// Фиксированное значение — менять нельзя, иначе `id` перестанут совпадать между прогонами.
const P916_ID_NAMESPACE: Uuid = Uuid::from_u128(0x8f2e_6b41_9a3d_4c17_b0e5_1d2f3a4b5c6d);

/// Детерминированный `id` строки движения из натурального ключа. `kind` — дискриминатор
/// движения ("order"/"cancel"/"buyout"/"return"/"marketing"), чтобы «заказ» и «отмена»
/// одного srid в один день не схлопнулись в один `id`. Одинаковый вход → одинаковый `id`
/// → повторная вставка перезаписывает строку (`on_conflict(id)`), а не задваивает обороты.
#[allow(clippy::too_many_arguments)]
fn deterministic_id(
    registrator_type: &str,
    registrator_ref: &str,
    stage: FunnelStage,
    kind: &str,
    cohort_date: &str,
    event_date: &str,
    connection_mp_ref: &str,
    nm_id: Option<i64>,
    marketplace_product_ref: Option<&str>,
) -> String {
    // Товарный ключ: nm_id (WB) или marketplace_product_ref (YM/OZON), иначе пусто.
    let product_key = nm_id
        .map(|v| v.to_string())
        .or_else(|| marketplace_product_ref.map(|s| s.to_string()))
        .unwrap_or_default();
    let key = format!(
        "{}|{}|{}|{}|{}|{}|{}|{}",
        registrator_type,
        registrator_ref,
        stage.as_str(),
        kind,
        cohort_date,
        event_date,
        connection_mp_ref,
        product_key
    );
    Uuid::new_v5(&P916_ID_NAMESPACE, key.as_bytes()).to_string()
}

/// Текущее время в MSK (+03:00) в формате RFC3339.
pub fn now_msk() -> String {
    let msk = FixedOffset::east_opt(3 * 3600).expect("valid MSK offset");
    Utc::now().with_timezone(&msk).to_rfc3339()
}

/// UTC-момент → MSK-дата `YYYY-MM-DD`. Публично — переиспользуется хуком a012 для
/// форматирования резолвнутой даты заказа (когорта выкупа/возврата).
pub fn msk_date_from_utc(dt: &DateTime<Utc>) -> String {
    let msk = FixedOffset::east_opt(3 * 3600).expect("valid MSK offset");
    dt.with_timezone(&msk).format("%Y-%m-%d").to_string()
}

/// Дата-строка источника → `YYYY-MM-DD` (первые 10 символов). Пустая → None.
fn msk_date_str(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.len() < 10 {
        return None;
    }
    Some(trimmed.chars().take(10).collect())
}

/// Заготовка строки движения: все метрики нулевые, показы (`show_*_count`) = None.
/// `kind` — дискриминатор движения для детерминированного `id` (см. `deterministic_id`).
#[allow(clippy::too_many_arguments)]
fn base_row(
    stage: FunnelStage,
    kind: &str,
    cohort_date: String,
    event_date: String,
    connection_mp_ref: String,
    marketplace_product_ref: Option<String>,
    nomenclature_ref: Option<String>,
    nm_id: Option<i64>,
    registrator_type: &str,
    registrator_ref: &str,
    now: &str,
) -> Model {
    let id = deterministic_id(
        registrator_type,
        registrator_ref,
        stage,
        kind,
        &cohort_date,
        &event_date,
        &connection_mp_ref,
        nm_id,
        marketplace_product_ref.as_deref(),
    );
    Model {
        id,
        stage: stage.as_str().to_string(),
        cohort_date,
        event_date,
        connection_mp_ref,
        marketplace_product_ref,
        nomenclature_ref,
        nm_id,
        registrator_type: registrator_type.to_string(),
        registrator_ref: registrator_ref.to_string(),
        order_key: None,
        show_free_count: None,
        show_paid_count: None,
        paid_open_count: None,
        paid_cart_count: None,
        total_impressions: None,
        open_count: 0,
        cart_count: 0,
        wishlist_count: 0,
        funnel_order_count: 0,
        funnel_order_sum: 0.0,
        funnel_cancel_count: None,
        funnel_cancel_sum: None,
        order_count: 0,
        order_sum: 0.0,
        cancel_count: 0,
        cancel_sum: 0.0,
        buyout_count: 0,
        buyout_sum: 0.0,
        return_count: 0,
        return_sum: 0.0,
        created_at_msk: now.to_string(),
        updated_at_msk: now.to_string(),
    }
}

/// Стадия 1: строки маркетинговой воронки из a036 — по одной на `line.nm_id`.
/// `cohort_date = event_date = header.document_date`. Показы (`show_free_count`/`show_paid_count`)
/// a036 не заполняет: `show_paid_count` — за a026 (реклама), `show_free_count` — источник
/// органических показов пока не подключён (a040 исключён), поэтому остаётся N/A.
/// Отмены пишутся в `funnel_cancel_*` (дневной счётчик маркетплейса), а не в `cancel_*`:
/// последние — order-level движения из a015 на дату отмены конкретного заказа.
pub fn from_wb_funnel_daily(doc: &WbSalesFunnelDaily, registrator_ref: &str) -> Vec<Model> {
    let now = now_msk();
    let Some(date) = msk_date_str(&doc.header.document_date) else {
        return Vec::new();
    };
    let connection = doc.header.connection_id.clone();

    let mut rows = Vec::new();
    for line in &doc.lines {
        let m = &line.metrics;
        // Разреженность: пропускаем товар без активности верха воронки.
        if m.open_count == 0
            && m.cart_count == 0
            && m.add_to_wishlist_count == 0
            && m.order_count == 0
            && m.order_sum == 0.0
            && m.cancel_count.unwrap_or(0) == 0
            && m.cancel_sum.unwrap_or(0.0) == 0.0
        {
            continue;
        }

        let mut row = base_row(
            FunnelStage::Marketing,
            "marketing",
            date.clone(),
            date.clone(),
            connection.clone(),
            None, // marketplace_product_ref в a036 отсутствует; мост по nm_id
            line.nomenclature_ref.clone(),
            Some(line.nm_id),
            REG_A036,
            registrator_ref,
            &now,
        );
        row.open_count = m.open_count;
        row.cart_count = m.cart_count;
        row.wishlist_count = m.add_to_wishlist_count;
        row.funnel_order_count = m.order_count;
        row.funnel_order_sum = m.order_sum;
        // Отмены по счётчику воронки маркетплейса — отдельная метрика от order-level
        // отмен a015 (те приходят строкой kind=cancel на дату отмены). None источника
        // пробрасываем как None: «счётчика не было» ≠ «отмен не было».
        row.funnel_cancel_count = m.cancel_count;
        row.funnel_cancel_sum = m.cancel_sum;
        rows.push(row);
    }
    rows
}

/// Стадия 1: платный ВЕРХ воронки из a026 (рекламный суточный отчёт) — по строке на `line.nm_id`.
/// Заполняются платные показы (`show_paid_count`=views), платные переходы
/// (`paid_open_count`=clicks) и платная корзина (`paid_cart_count`=atbs). Органика — за a040,
/// суммарные переходы/корзина — за a036. Каждая метрика nullable: пишем `Some` только при >0
/// (N/A ≠ 0). Один документ a026 = один advert_id × дата, строки — nm_id; на чтении SUM
/// складывает по всем кампаниям дня. `cohort_date = event_date = document_date`.
pub fn from_wb_advert_daily(doc: &WbAdvertDaily, registrator_ref: &str) -> Vec<Model> {
    let now = now_msk();
    let Some(date) = msk_date_str(&doc.header.document_date) else {
        return Vec::new();
    };
    let connection = doc.header.connection_id.clone();

    let mut rows = Vec::new();
    for line in &doc.lines {
        let m = &line.metrics;
        // Разреженность: пропускаем товар без платной активности верха воронки.
        if m.views == 0 && m.clicks == 0 && m.atbs == 0 {
            continue;
        }
        let mut row = base_row(
            FunnelStage::Marketing,
            "marketing",
            date.clone(),
            date.clone(),
            connection.clone(),
            None,
            line.nomenclature_ref.clone(),
            Some(line.nm_id),
            REG_A026,
            registrator_ref,
            &now,
        );
        row.show_paid_count = (m.views > 0).then_some(m.views);
        row.paid_open_count = (m.clicks > 0).then_some(m.clicks);
        row.paid_cart_count = (m.atbs > 0).then_some(m.atbs);
        rows.push(row);
    }
    rows
}

/// Стадия 2: движение заказа из a015. Всегда одна строка «заказ» на дату заказа;
/// при отмене — дополнительная строка «отмена» на дату отмены (когорта = дата заказа).
pub fn from_wb_orders(doc: &WbOrders, registrator_ref: &str) -> Vec<Model> {
    let now = now_msk();
    let connection = doc.header.connection_id.clone();
    let order_date = msk_date_from_utc(&doc.state.order_dt);
    let amount = doc.line.allocation_basis();
    let mp_ref = doc.marketplace_product_ref.clone();
    let nom_ref = doc.nomenclature_ref.clone();
    let nm_id = Some(doc.line.nm_id);
    // srid — мост к атрибуции рекламы p913 (канальный сплит заказа/отмены на чтении).
    let order_key = Some(doc.header.document_no.clone());

    let mut rows = Vec::new();

    // Строка «заказ»: обе оси = дата заказа.
    let mut order_row = base_row(
        FunnelStage::Fulfillment,
        "order",
        order_date.clone(),
        order_date.clone(),
        connection.clone(),
        mp_ref.clone(),
        nom_ref.clone(),
        nm_id,
        REG_A015,
        registrator_ref,
        &now,
    );
    order_row.order_key = order_key.clone();
    order_row.order_count = 1;
    order_row.order_sum = amount;
    rows.push(order_row);

    // Строка «отмена»: когорта = дата заказа, событие = дата отмены.
    // Фолбэк-цепочка: cancel_dt → last_change_dt → дата заказа. last_change_dt (Statistics
    // API) отражает момент смены статуса, поэтому для заказов без точной cancel_dt он
    // на порядок ближе к правде, чем дата заказа: иначе отмена садится на день заказа
    // и потоковая ось показывает всплеск отмен там, где их не было.
    if doc.state.is_cancel {
        let cancel_event_date = doc
            .state
            .cancel_dt
            .as_ref()
            .or(doc.state.last_change_dt.as_ref())
            .map(msk_date_from_utc)
            .unwrap_or_else(|| order_date.clone());
        let mut cancel_row = base_row(
            FunnelStage::Fulfillment,
            "cancel",
            order_date.clone(),
            cancel_event_date,
            connection.clone(),
            mp_ref.clone(),
            nom_ref.clone(),
            nm_id,
            REG_A015,
            registrator_ref,
            &now,
        );
        cancel_row.order_key = order_key.clone();
        cancel_row.cancel_count = 1;
        cancel_row.cancel_sum = amount;
        rows.push(cancel_row);
    }

    rows
}

/// Сумма продажи/возврата a012: `amount_line` → `sell_out_fact` → `finished_price * qty`.
fn sales_amount(doc: &WbSales) -> f64 {
    let line = &doc.line;
    if let Some(v) = line.amount_line.filter(|v| *v != 0.0) {
        return v;
    }
    if let Some(v) = line.sell_out_fact.filter(|v| *v != 0.0) {
        return v;
    }
    line.finished_price.unwrap_or(0.0) * line.qty
}

/// Стадия 2: движение выкупа/возврата из a012. Одна строка.
/// `event_date` = дата продажи (`sale_dt`). `cohort_date` = `order_cohort_date`, если известна
/// (дата заказа из a015 по srid — резолвится вызывающей стороной, где доступна БД), иначе
/// фолбэком = дата продажи (для срезов, где заказ не найден).
pub fn from_wb_sales(
    doc: &WbSales,
    registrator_ref: &str,
    order_cohort_date: Option<String>,
) -> Vec<Model> {
    let now = now_msk();
    let connection = doc.header.connection_id.clone();
    let sale_date = msk_date_from_utc(&doc.state.sale_dt);
    let cohort_date = order_cohort_date.unwrap_or_else(|| sale_date.clone());
    let amount = sales_amount(doc);
    let count = doc.line.qty.round() as i64;
    if count == 0 && amount == 0.0 {
        return Vec::new();
    }

    let is_return = doc.is_customer_return || doc.state.event_type.eq_ignore_ascii_case("return");
    let kind = if is_return { "return" } else { "buyout" };

    let mut row = base_row(
        FunnelStage::Fulfillment,
        kind,
        cohort_date,
        sale_date,
        connection,
        doc.marketplace_product_ref.clone(),
        doc.nomenclature_ref.clone(),
        Some(doc.line.nm_id),
        REG_A012,
        registrator_ref,
        &now,
    );
    // srid — мост к атрибуции рекламы p913 (канальный сплит выкупа/возврата на чтении).
    row.order_key = Some(doc.header.document_no.clone());
    if is_return {
        // Возвраты приходят из a012 со знаком минус (qty/forPay отрицательные), но в p916
        // все метрики — положительные величины: потребитель вычитает возвраты явно.
        // Без abs() SUM(return_count) выходил отрицательным и «выкупы минус возвраты»
        // молча превращалось в сложение.
        row.return_count = count.abs();
        row.return_sum = amount.abs();
    } else {
        row.buyout_count = count;
        row.buyout_sum = amount;
    }
    vec![row]
}

/// Стадия 1 для YM: верх воронки из a041 (отчёт «Аналитика продаж») — по строке на товар.
/// `cohort_date = event_date = день отчёта`.
///
/// Маппинг метрик:
/// - `shows` → `total_impressions` (настоящие показы; у WB эта колонка пуста —
///   там счётчика органических показов нет, а `show_paid_count` — только реклама);
/// - `clicks` → `open_count` (переходы в карточку, как у a036);
/// - `to_cart` → `cart_count`;
/// - `order_items` → `funnel_order_count`, `order_sum` → `funnel_order_sum`,
///   `canceled_count` → `funnel_cancel_count`
///   (счётчики маркетплейса, отличны от фактических заказов/отмен стадии fulfillment).
///
/// `by_msku_shows` (показы всех продавцов по MSKU) НЕ проецируется: это объём рынка,
/// делить на него свои клики нельзя. `delivered_count`/`delivered_sum`/`returned_count`
/// тоже остаются в a041 — низ воронки берётся из документов a013/a016, как у WB из a015/a012.
/// `order_sum` → `funnel_order_sum` (целые рубли из `orderItemsTotalAmount`).
pub fn from_ym_shows_sales_daily(
    doc: &contracts::domain::a041_ym_shows_sales_daily::aggregate::YmShowsSalesDaily,
    registrator_ref: &str,
) -> Vec<Model> {
    let now = now_msk();
    let Some(date) = msk_date_str(&doc.header.document_date) else {
        return Vec::new();
    };
    let connection = doc.header.connection_id.clone();

    let mut rows = Vec::new();
    for line in &doc.lines {
        let m = &line.metrics;
        if !m.has_activity() {
            continue;
        }
        let mut row = base_row(
            FunnelStage::Marketing,
            "marketing",
            date.clone(),
            date.clone(),
            connection.clone(),
            line.marketplace_product_ref.clone(),
            line.nomenclature_ref.clone(),
            None, // nm_id — WB-специфичен
            REG_A041,
            registrator_ref,
            &now,
        );
        row.total_impressions = m.shows;
        row.open_count = m.clicks.unwrap_or(0);
        row.cart_count = m.to_cart.unwrap_or(0);
        row.funnel_order_count = m.order_items.unwrap_or(0);
        row.funnel_order_sum = m.order_sum.unwrap_or(0) as f64;
        row.funnel_cancel_count = m.canceled_count;
        rows.push(row);
    }
    rows
}

// ─────────────────────────────────────────────────────────────────────────────
// Yandex Market, стадия 2 (fulfillment)
//
// Матрица источников — чтобы одно физическое событие не посчиталось дважды:
//   заказ / отмена / выкуп — только a013 (документ заказа со статусами);
//   возврат               — только a016, и только `return_type = RETURN`.
// `UNREDEEMED` из a016 (невыкуп) НЕ проводится: то же событие приходит из a013
// как отмена или как позиция REJECTED. Детали `RETURNED` внутри a013 тоже не
// проводятся — по возвратам главнее a016.
//
// Единица измерения — штуки позиции (`line.qty`), в отличие от WB, где документ
// заказа сам по себе одна единица.
// ─────────────────────────────────────────────────────────────────────────────

/// Цена за единицу позиции YM: `amount_line / qty` → `price_effective` → `buyer_price`.
/// Нужна, чтобы частичный отказ (2 из 5 шт) дал корректную сумму, а не сумму всей строки.
fn ym_unit_price(line: &contracts::domain::a013_ym_order::aggregate::YmOrderLine) -> f64 {
    if let Some(amount) = line.amount_line.filter(|v| *v != 0.0) {
        if line.qty != 0.0 {
            return amount / line.qty;
        }
    }
    line.price_effective
        .filter(|v| *v != 0.0)
        .or(line.buyer_price)
        .unwrap_or(0.0)
}

/// Сумма единиц позиции с указанным статусом судьбы (`items[].details[].itemStatus`).
fn ym_detail_units(
    line: &contracts::domain::a013_ym_order::aggregate::YmOrderLine,
    status: &str,
) -> f64 {
    line.details
        .iter()
        .filter(|d| d.status.eq_ignore_ascii_case(status))
        .map(|d| d.count)
        .sum()
}

/// Стадия 2 для YM: движения заказа из a013 — по строке на позицию.
///
/// - «заказ» — всегда, на дату создания заказа (обе оси);
/// - «отмена» — при `status_norm = CANCELLED` на всё количество, а у доставленных
///   заказов на количество позиций со статусом `REJECTED` (отказ при получении);
/// - «выкуп» — при `status_norm = DELIVERED` на количество за вычетом `REJECTED`
///   (отказанные единицы физически не выкупались). Единицы `RETURNED` из выкупа НЕ
///   вычитаются: они были выкуплены, а возврат придёт отдельным движением из a016.
pub fn from_ym_order(doc: &YmOrder, registrator_ref: &str) -> Vec<Model> {
    let now = now_msk();
    let connection = doc.header.connection_id.clone();
    let Some(created) = doc.state.creation_date.as_ref() else {
        // Без даты создания когорту не построить — движения не пишем (документ
        // попадёт в quality-check как отсутствующий в проекции).
        return Vec::new();
    };
    let order_date = msk_date_from_utc(created);
    let order_key = Some(doc.header.document_no.clone());

    let status_changed_date = doc
        .state
        .status_changed_at
        .as_ref()
        .or(doc.state.updated_at_source.as_ref())
        .map(msk_date_from_utc)
        .unwrap_or_else(|| order_date.clone());
    let delivery_date = doc
        .state
        .delivery_date
        .as_ref()
        .map(msk_date_from_utc)
        .unwrap_or_else(|| status_changed_date.clone());

    let is_cancelled = doc.state.status_norm.eq_ignore_ascii_case("CANCELLED");
    let is_delivered = doc.state.status_norm.eq_ignore_ascii_case("DELIVERED");

    let mut rows = Vec::new();
    for line in &doc.lines {
        if line.qty == 0.0 {
            continue;
        }
        let unit_price = ym_unit_price(line);
        let qty = line.qty.round() as i64;
        let mp_ref = line.marketplace_product_ref.clone();
        let nom_ref = line.nomenclature_ref.clone();

        let make = |kind: &str, event_date: String| -> Model {
            let mut row = base_row(
                FunnelStage::Fulfillment,
                kind,
                order_date.clone(),
                event_date,
                connection.clone(),
                mp_ref.clone(),
                nom_ref.clone(),
                None, // nm_id — WB-специфичен; товарный ключ YM = marketplace_product_ref
                REG_A013,
                registrator_ref,
                &now,
            );
            row.order_key = order_key.clone();
            row
        };

        let mut order_row = make("order", order_date.clone());
        order_row.order_count = qty;
        order_row.order_sum = unit_price * line.qty;
        rows.push(order_row);

        let rejected_units = ym_detail_units(line, "REJECTED");

        if is_cancelled {
            let mut cancel_row = make("cancel", status_changed_date.clone());
            cancel_row.cancel_count = qty;
            cancel_row.cancel_sum = unit_price * line.qty;
            rows.push(cancel_row);
            continue;
        }

        if rejected_units > 0.0 {
            // Дата отказа берётся из самой детали: она точнее общей смены статуса.
            let reject_date = line
                .details
                .iter()
                .filter(|d| d.status.eq_ignore_ascii_case("REJECTED"))
                .find_map(|d| d.update_date.as_deref().and_then(msk_date_str))
                .unwrap_or_else(|| delivery_date.clone());
            let mut cancel_row = make("cancel", reject_date);
            cancel_row.cancel_count = rejected_units.round() as i64;
            cancel_row.cancel_sum = unit_price * rejected_units;
            rows.push(cancel_row);
        }

        if is_delivered {
            let bought_units = line.qty - rejected_units;
            if bought_units > 0.0 {
                let mut buyout_row = make("buyout", delivery_date.clone());
                buyout_row.buyout_count = bought_units.round() as i64;
                buyout_row.buyout_sum = unit_price * bought_units;
                rows.push(buyout_row);
            }
        }
    }

    rows
}

/// Стадия 2 для YM: возвраты из a016 — по строке на позицию возврата.
///
/// Проводится только `return_type = RETURN` (возврат после получения). `UNREDEEMED`
/// (невыкуп) пропускается: это то же событие, что отмена/`REJECTED` из a013, и его
/// проведение задвоило бы отказы.
///
/// `cohort_date` — дата заказа, резолвится вызывающей стороной по `header.order_id`
/// (там доступна БД); без неё когорта фолбэком = дата события возврата.
/// `product_refs` — соответствие `shop_sku → (a007, a004)`, тоже резолвится снаружи:
/// строки a016 ссылок на товар не несут.
pub fn from_ym_return(
    doc: &YmReturn,
    registrator_ref: &str,
    order_cohort_date: Option<String>,
    product_refs: &std::collections::HashMap<String, (Option<String>, Option<String>)>,
) -> Vec<Model> {
    if !doc.header.return_type.eq_ignore_ascii_case("RETURN") {
        return Vec::new();
    }

    let now = now_msk();
    let connection = doc.header.connection_id.clone();
    let event_date = doc
        .state
        .refund_date
        .as_ref()
        .or(doc.state.updated_at_source.as_ref())
        .or(doc.state.created_at_source.as_ref())
        .map(msk_date_from_utc);
    let Some(event_date) = event_date else {
        return Vec::new();
    };
    let cohort_date = order_cohort_date.unwrap_or_else(|| event_date.clone());
    let order_key = Some(doc.header.order_id.to_string());

    let mut rows = Vec::new();
    for line in &doc.lines {
        if line.count == 0 {
            continue;
        }
        let (mp_ref, nom_ref) = product_refs
            .get(&line.shop_sku)
            .cloned()
            .unwrap_or((None, None));

        let mut row = base_row(
            FunnelStage::Fulfillment,
            "return",
            cohort_date.clone(),
            event_date.clone(),
            connection.clone(),
            mp_ref,
            nom_ref,
            None,
            REG_A016,
            registrator_ref,
            &now,
        );
        row.order_key = order_key.clone();
        // Положительные величины — общее правило p916 (см. модуль-док).
        row.return_count = (line.count as i64).abs();
        row.return_sum = (line.price.unwrap_or(0.0) * line.count as f64).abs();
        rows.push(row);
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};
    use contracts::domain::a012_wb_sales::aggregate::{
        WbSales, WbSalesHeader, WbSalesLine, WbSalesSourceMeta, WbSalesState, WbSalesWarehouse,
    };
    use contracts::domain::a015_wb_orders::aggregate::{
        WbOrders, WbOrdersGeography, WbOrdersHeader, WbOrdersLine, WbOrdersSourceMeta,
        WbOrdersState, WbOrdersWarehouse,
    };
    use contracts::domain::a026_wb_advert_daily::aggregate::{
        WbAdvertDailyHeader, WbAdvertDailyLine, WbAdvertDailyMetrics, WbAdvertDailySourceMeta,
    };
    use contracts::domain::a036_wb_sales_funnel_daily::aggregate::{
        WbSalesFunnelDailyHeader, WbSalesFunnelDailyLine, WbSalesFunnelDailyMetrics,
        WbSalesFunnelDailySourceMeta,
    };
    use contracts::domain::a041_ym_shows_sales_daily::aggregate::{
        YmShowsSalesDailyHeader, YmShowsSalesDailyLine, YmShowsSalesDailyMetrics,
        YmShowsSalesDailySourceMeta,
    };

    fn utc(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    fn wb_order_with_last_change(
        is_cancel: bool,
        cancel_dt: Option<&str>,
        last_change_dt: Option<&str>,
    ) -> WbOrders {
        let mut doc = wb_order(is_cancel, cancel_dt);
        doc.state.last_change_dt = last_change_dt.map(utc);
        doc
    }

    fn wb_order(is_cancel: bool, cancel_dt: Option<&str>) -> WbOrders {
        let line = WbOrdersLine {
            line_id: "srid-1".to_string(),
            supplier_article: "ART-1".to_string(),
            nm_id: 777,
            barcode: "bc".to_string(),
            category: None,
            subject: None,
            brand: None,
            tech_size: None,
            qty: 1.0,
            total_price: Some(1200.0),
            discount_percent: None,
            spp: None,
            finished_price: Some(900.0),
            price_with_disc: Some(1000.0),
            price: None,
            sale_price: None,
            dealer_price_ut: None,
            margin_pro: None,
            currency_code: None,
            fx_rate: None,
        };
        let state = WbOrdersState {
            order_dt: utc("2026-03-01T05:00:00Z"),
            last_change_dt: None,
            is_cancel,
            cancel_dt: cancel_dt.map(utc),
            is_supply: None,
            is_realization: None,
        };
        let header = WbOrdersHeader {
            document_no: "srid-1".to_string(),
            connection_id: "conn-1".to_string(),
            organization_id: "org-1".to_string(),
            marketplace_id: "mp-1".to_string(),
        };
        let source_meta = WbOrdersSourceMeta {
            income_id: None,
            sticker: None,
            g_number: None,
            raw_payload_ref: "raw-1".to_string(),
            marketplace_raw_payload_ref: None,
            fetched_at: utc("2026-03-01T06:00:00Z"),
            document_version: 1,
        };
        let mut doc = WbOrders::new_for_insert(
            "code-1".to_string(),
            "WB заказ".to_string(),
            header,
            line,
            state,
            WbOrdersWarehouse {
                warehouse_name: None,
                warehouse_type: None,
            },
            WbOrdersGeography {
                country_name: None,
                oblast_okrug_name: None,
                region_name: None,
            },
            source_meta,
            true,
            Some("2026-03-01".to_string()),
        );
        doc.nomenclature_ref = Some("nom-1".to_string());
        doc.marketplace_product_ref = Some("mp-prod-1".to_string());
        doc
    }

    #[test]
    fn order_without_cancel_emits_single_order_row() {
        let doc = wb_order(false, None);
        let rows = from_wb_orders(&doc, "reg-1");
        assert_eq!(rows.len(), 1);
        let r = &rows[0];
        assert_eq!(r.stage, "fulfillment");
        assert_eq!(r.order_count, 1);
        assert_eq!(r.order_sum, 1000.0); // price_with_disc через allocation_basis
        assert_eq!(r.cohort_date, "2026-03-01");
        assert_eq!(r.event_date, "2026-03-01");
        assert_eq!(r.cancel_count, 0);
        assert_eq!(r.nm_id, Some(777));
        assert_eq!(r.order_key.as_deref(), Some("srid-1")); // мост к p913
    }

    #[test]
    fn cancelled_order_emits_order_and_cancel_rows_on_split_dates() {
        let doc = wb_order(true, Some("2026-03-05T22:00:00Z"));
        let rows = from_wb_orders(&doc, "reg-1");
        assert_eq!(rows.len(), 2);

        let order = rows.iter().find(|r| r.order_count == 1).unwrap();
        assert_eq!(order.cancel_count, 0);
        assert_eq!(order.cohort_date, "2026-03-01");
        assert_eq!(order.event_date, "2026-03-01");

        let cancel = rows.iter().find(|r| r.cancel_count == 1).unwrap();
        assert_eq!(cancel.order_count, 0);
        // Когорта = дата заказа, событие = дата отмены (MSK +3 → 2026-03-06).
        assert_eq!(cancel.cohort_date, "2026-03-01");
        assert_eq!(cancel.event_date, "2026-03-06");
        assert_eq!(cancel.cancel_sum, 1000.0);
    }

    #[test]
    fn cancel_without_cancel_dt_falls_back_to_last_change_date() {
        // FBS-заказ: точной даты отмены нет, но есть момент смены статуса.
        let doc = wb_order_with_last_change(true, None, Some("2026-03-07T21:30:00Z"));
        let rows = from_wb_orders(&doc, "reg-1");
        let cancel = rows.iter().find(|r| r.cancel_count == 1).unwrap();
        assert_eq!(cancel.cohort_date, "2026-03-01");
        // MSK +3 → 2026-03-08, а не дата заказа.
        assert_eq!(cancel.event_date, "2026-03-08");
    }

    #[test]
    fn cancel_without_any_date_falls_back_to_order_date() {
        let doc = wb_order_with_last_change(true, None, None);
        let rows = from_wb_orders(&doc, "reg-1");
        let cancel = rows.iter().find(|r| r.cancel_count == 1).unwrap();
        assert_eq!(cancel.event_date, "2026-03-01");
    }

    #[test]
    fn return_metrics_are_stored_as_positive_magnitudes() {
        // a012 отдаёт возврат отрицательными qty/суммой; в p916 знак несёт вид движения.
        let mut doc = wb_sale(true, "2026-03-12T08:00:00Z");
        doc.line.qty = -2.0;
        doc.line.amount_line = Some(-1000.0);
        let rows = from_wb_sales(&doc, "reg-s-2", Some("2026-03-01".to_string()));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].return_count, 2);
        assert_eq!(rows[0].return_sum, 1000.0);
    }

    #[test]
    fn funnel_daily_skips_empty_and_maps_metrics() {
        let header = WbSalesFunnelDailyHeader {
            document_no: "F-1".to_string(),
            document_date: "2026-03-02".to_string(),
            connection_id: "conn-1".to_string(),
            organization_id: "org-1".to_string(),
            marketplace_id: "mp-1".to_string(),
            currency: "RUB".to_string(),
        };
        let active_line = WbSalesFunnelDailyLine {
            nm_id: 101,
            title: "T".to_string(),
            vendor_code: "V".to_string(),
            brand_name: "B".to_string(),
            subject_id: 1,
            subject_name: "S".to_string(),
            nomenclature_ref: Some("nom-101".to_string()),
            metrics: WbSalesFunnelDailyMetrics {
                open_count: 10,
                cart_count: 4,
                order_count: 2,
                order_sum: 3000.0,
                add_to_wishlist_count: 1,
                ..Default::default()
            },
        };
        let empty_line = WbSalesFunnelDailyLine {
            nm_id: 202,
            title: "T2".to_string(),
            vendor_code: "V2".to_string(),
            brand_name: "B2".to_string(),
            subject_id: 2,
            subject_name: "S2".to_string(),
            nomenclature_ref: None,
            metrics: WbSalesFunnelDailyMetrics::default(),
        };
        let doc = contracts::domain::a036_wb_sales_funnel_daily::aggregate::WbSalesFunnelDaily::new_for_insert(
            header,
            WbSalesFunnelDailyMetrics::default(),
            vec![active_line, empty_line],
            WbSalesFunnelDailySourceMeta {
                source: "wb".to_string(),
                fetched_at: "2026-03-02T00:00:00Z".to_string(),
            },
        );

        let rows = from_wb_funnel_daily(&doc, "reg-f-1");
        assert_eq!(rows.len(), 1); // пустая строка отброшена
        let r = &rows[0];
        assert_eq!(r.stage, "marketing");
        assert_eq!(r.nm_id, Some(101));
        assert_eq!(r.open_count, 10);
        assert_eq!(r.cart_count, 4);
        assert_eq!(r.wishlist_count, 1);
        assert_eq!(r.funnel_order_count, 2);
        assert_eq!(r.funnel_order_sum, 3000.0);
        assert_eq!(r.cohort_date, "2026-03-02");
        assert_eq!(r.event_date, "2026-03-02");
        assert!(r.show_free_count.is_none());
        assert!(r.show_paid_count.is_none());
        // Источник счётчик отмен не отдал → N/A, а не 0.
        assert!(r.funnel_cancel_count.is_none());
    }

    #[test]
    fn funnel_daily_maps_cancel_counter_and_keeps_cancel_only_line() {
        let header = WbSalesFunnelDailyHeader {
            document_no: "F-2".to_string(),
            document_date: "2026-03-04".to_string(),
            connection_id: "conn-1".to_string(),
            organization_id: "org-1".to_string(),
            marketplace_id: "mp-1".to_string(),
            currency: "RUB".to_string(),
        };
        // День, где у товара из активности только отмены (заказ пришёл в прошлый период).
        let cancel_only_line = WbSalesFunnelDailyLine {
            nm_id: 303,
            title: "T".to_string(),
            vendor_code: "V".to_string(),
            brand_name: "B".to_string(),
            subject_id: 1,
            subject_name: "S".to_string(),
            nomenclature_ref: None,
            metrics: WbSalesFunnelDailyMetrics {
                cancel_count: Some(3),
                cancel_sum: Some(4500.0),
                ..Default::default()
            },
        };
        let doc = contracts::domain::a036_wb_sales_funnel_daily::aggregate::WbSalesFunnelDaily::new_for_insert(
            header,
            WbSalesFunnelDailyMetrics::default(),
            vec![cancel_only_line],
            WbSalesFunnelDailySourceMeta {
                source: "wb_detail_history_report".to_string(),
                fetched_at: "2026-03-04T00:00:00Z".to_string(),
            },
        );

        let rows = from_wb_funnel_daily(&doc, "reg-f-2");
        assert_eq!(rows.len(), 1); // строка с одними отменами не считается пустой
        let r = &rows[0];
        assert_eq!(r.funnel_cancel_count, Some(3));
        assert_eq!(r.funnel_cancel_sum, Some(4500.0));
        // Счётчик воронки не подменяет order-level отмены из a015.
        assert_eq!(r.cancel_count, 0);
        assert_eq!(r.stage, "marketing");
    }

    #[test]
    fn advert_daily_maps_only_paid_shows_and_skips_zero_views() {
        let header = WbAdvertDailyHeader {
            document_no: "AD-1".to_string(),
            document_date: "2026-03-03".to_string(),
            advert_id: 555,
            connection_id: "conn-1".to_string(),
            organization_id: "org-1".to_string(),
            marketplace_id: "mp-1".to_string(),
        };
        let paid_line = WbAdvertDailyLine {
            nm_id: 303,
            nm_name: "N".to_string(),
            nomenclature_ref: Some("nom-303".to_string()),
            advert_ids: vec![555],
            app_types: vec![],
            placements: vec![],
            metrics: WbAdvertDailyMetrics {
                views: 1200,
                clicks: 30,
                ..Default::default()
            },
        };
        let zero_line = WbAdvertDailyLine {
            nm_id: 404,
            nm_name: "Z".to_string(),
            nomenclature_ref: None,
            advert_ids: vec![555],
            app_types: vec![],
            placements: vec![],
            metrics: WbAdvertDailyMetrics::default(),
        };
        let doc = WbAdvertDaily::new_for_insert(
            header,
            WbAdvertDailyMetrics::default(),
            WbAdvertDailyMetrics::default(),
            vec![paid_line, zero_line],
            WbAdvertDailySourceMeta {
                source: "wb_advert_stats".to_string(),
                fetched_at: "2026-03-03T00:00:00Z".to_string(),
            },
        );

        let rows = from_wb_advert_daily(&doc, "reg-ad-1");
        assert_eq!(rows.len(), 1); // строка без показов отброшена
        let r = &rows[0];
        assert_eq!(r.stage, "marketing");
        assert_eq!(r.registrator_type, REG_A026);
        assert_eq!(r.nm_id, Some(303));
        assert_eq!(r.show_paid_count, Some(1200));
        assert_eq!(r.paid_open_count, Some(30)); // платные переходы = clicks
        assert!(r.paid_cart_count.is_none()); // atbs=0 → None (N/A ≠ 0)
        assert!(r.show_free_count.is_none()); // органика — за a040
        assert_eq!(r.open_count, 0); // суммарные переходы/корзина — не из a026
        assert!(r.order_key.is_none()); // marketing-строка без srid
        assert_eq!(r.cohort_date, "2026-03-03");
        assert_eq!(r.event_date, "2026-03-03");
    }

    fn wb_sale(is_return: bool, sale_dt: &str) -> WbSales {
        let line = WbSalesLine {
            line_id: "srid-9".to_string(),
            supplier_article: "ART-9".to_string(),
            nm_id: 888,
            barcode: "bc9".to_string(),
            name: "Товар 9".to_string(),
            qty: 1.0,
            price_list: None,
            discount_total: None,
            price_effective: None,
            amount_line: Some(500.0),
            currency_code: None,
            total_price: None,
            payment_sale_amount: None,
            discount_percent: None,
            spp: None,
            finished_price: Some(500.0),
            is_fact: None,
            sell_out_plan: None,
            sell_out_fact: None,
            acquiring_fee_plan: None,
            acquiring_fee_fact: None,
            other_fee_plan: None,
            other_fee_fact: None,
            supplier_payout_plan: None,
            supplier_payout_fact: None,
            profit_plan: None,
            profit_fact: None,
            cost_of_production: None,
            commission_plan: None,
            commission_fact: None,
            dealer_price_ut: None,
        };
        let state = WbSalesState {
            event_type: if is_return { "return" } else { "sale" }.to_string(),
            status_norm: "DELIVERED".to_string(),
            sale_dt: utc(sale_dt),
            last_change_dt: None,
            is_supply: None,
            is_realization: None,
        };
        let header = WbSalesHeader {
            document_no: "srid-9".to_string(),
            sale_id: Some("S9".to_string()),
            connection_id: "conn-1".to_string(),
            organization_id: "org-1".to_string(),
            marketplace_id: "mp-1".to_string(),
        };
        let source_meta = WbSalesSourceMeta {
            raw_payload_ref: "raw-9".to_string(),
            fetched_at: utc("2026-03-10T06:00:00Z"),
            document_version: 1,
        };
        let mut doc = WbSales::new_for_insert(
            "code-9".to_string(),
            "WB продажа".to_string(),
            header,
            line,
            state,
            WbSalesWarehouse {
                warehouse_name: None,
                warehouse_type: None,
            },
            source_meta,
            true,
        );
        doc.is_customer_return = is_return;
        doc.nomenclature_ref = Some("nom-9".to_string());
        doc.marketplace_product_ref = Some("mp-prod-9".to_string());
        doc
    }

    #[test]
    fn sales_uses_order_cohort_date_when_provided() {
        let doc = wb_sale(false, "2026-03-10T08:00:00Z");
        let rows = from_wb_sales(&doc, "reg-s-1", Some("2026-03-01".to_string()));
        assert_eq!(rows.len(), 1);
        let r = &rows[0];
        assert_eq!(r.buyout_count, 1);
        assert_eq!(r.order_key.as_deref(), Some("srid-9")); // мост к p913
                                                            // Когорта = дата заказа (передана извне), событие = дата продажи (MSK +3).
        assert_eq!(r.cohort_date, "2026-03-01");
        assert_eq!(r.event_date, "2026-03-10");
    }

    #[test]
    fn sales_falls_back_to_sale_date_without_order() {
        let doc = wb_sale(true, "2026-03-10T08:00:00Z");
        let rows = from_wb_sales(&doc, "reg-s-1", None);
        assert_eq!(rows.len(), 1);
        let r = &rows[0];
        assert_eq!(r.return_count, 1);
        // Заказ не найден → когорта фолбэком = дата продажи.
        assert_eq!(r.cohort_date, "2026-03-10");
        assert_eq!(r.event_date, "2026-03-10");
    }

    fn ym_order(status_norm: &str, qty: f64, details: Vec<(&str, f64, &str)>) -> YmOrder {
        use contracts::domain::a013_ym_order::aggregate::{
            YmOrder, YmOrderHeader, YmOrderLine, YmOrderLineDetail, YmOrderSourceMeta, YmOrderState,
        };

        let line = YmOrderLine {
            line_id: "item-1".to_string(),
            shop_sku: "SKU-1".to_string(),
            offer_id: "OFF-1".to_string(),
            name: "Товар".to_string(),
            qty,
            price_list: None,
            discount_total: None,
            price_effective: Some(500.0),
            amount_line: Some(500.0 * qty),
            currency_code: Some("RUR".to_string()),
            buyer_price: None,
            subsidies_json: None,
            status: None,
            price_plan: None,
            marketplace_product_ref: Some("mp-ym-1".to_string()),
            nomenclature_ref: Some("nom-ym-1".to_string()),
            dealer_price_ut: None,
            details: details
                .into_iter()
                .map(|(status, count, update_date)| YmOrderLineDetail {
                    count,
                    status: status.to_string(),
                    update_date: Some(update_date.to_string()),
                })
                .collect(),
        };
        let header = YmOrderHeader {
            document_no: "YM-1001".to_string(),
            connection_id: "conn-ym".to_string(),
            organization_id: "org-1".to_string(),
            marketplace_id: "mp-ym".to_string(),
            campaign_id: "camp-1".to_string(),
            fulfillment_type: Some("FBS".to_string()),
            total_amount: Some(500.0 * qty),
            currency: Some("RUR".to_string()),
            items_total: None,
            delivery_total: None,
            subsidies_json: None,
            total_dealer_amount: None,
            margin_pro: None,
        };
        let state = YmOrderState {
            status_raw: status_norm.to_string(),
            substatus_raw: None,
            status_norm: status_norm.to_string(),
            status_changed_at: Some(utc("2026-04-05T09:00:00Z")),
            updated_at_source: Some(utc("2026-04-05T09:00:00Z")),
            creation_date: Some(utc("2026-04-01T10:00:00Z")),
            delivery_date: Some(utc("2026-04-07T12:00:00Z")),
        };
        let source_meta = YmOrderSourceMeta {
            raw_payload_ref: "raw-ym-1".to_string(),
            fetched_at: utc("2026-04-08T00:00:00Z"),
            document_version: 1,
        };
        YmOrder::new_for_insert(
            "YM-1001".to_string(),
            "YM заказ".to_string(),
            header,
            vec![line],
            state,
            source_meta,
            true,
        )
    }

    #[test]
    fn ym_delivered_order_emits_order_and_buyout() {
        let doc = ym_order("DELIVERED", 3.0, vec![]);
        let rows = from_ym_order(&doc, "reg-ym-1");
        assert_eq!(rows.len(), 2);

        let order = rows.iter().find(|r| r.order_count > 0).unwrap();
        assert_eq!(order.order_count, 3);
        assert_eq!(order.cohort_date, "2026-04-01");
        assert_eq!(order.event_date, "2026-04-01");
        assert_eq!(order.registrator_type, REG_A013);
        assert!(order.nm_id.is_none()); // nm_id — WB-специфичен
        assert_eq!(order.marketplace_product_ref.as_deref(), Some("mp-ym-1"));

        let buyout = rows.iter().find(|r| r.buyout_count > 0).unwrap();
        assert_eq!(buyout.buyout_count, 3);
        assert_eq!(buyout.cohort_date, "2026-04-01");
        assert_eq!(buyout.event_date, "2026-04-07"); // дата доставки
    }

    #[test]
    fn ym_cancelled_order_emits_cancel_on_status_change_date() {
        let doc = ym_order("CANCELLED", 2.0, vec![]);
        let rows = from_ym_order(&doc, "reg-ym-2");
        assert_eq!(rows.len(), 2);

        let cancel = rows.iter().find(|r| r.cancel_count > 0).unwrap();
        assert_eq!(cancel.cancel_count, 2);
        assert_eq!(cancel.cancel_sum, 1000.0);
        assert_eq!(cancel.cohort_date, "2026-04-01");
        assert_eq!(cancel.event_date, "2026-04-05");
        // Отменённый заказ не даёт выкупа.
        assert!(rows.iter().all(|r| r.buyout_count == 0));
    }

    #[test]
    fn ym_partial_rejection_splits_cancel_and_buyout() {
        // 5 заказано, 2 отказано при получении → выкуплено 3.
        let doc = ym_order(
            "DELIVERED",
            5.0,
            vec![("REJECTED", 2.0, "2026-04-08T15:00:00Z")],
        );
        let rows = from_ym_order(&doc, "reg-ym-3");
        assert_eq!(rows.len(), 3);

        let cancel = rows.iter().find(|r| r.cancel_count > 0).unwrap();
        assert_eq!(cancel.cancel_count, 2);
        assert_eq!(cancel.cancel_sum, 1000.0);
        assert_eq!(cancel.event_date, "2026-04-08"); // дата из details

        let buyout = rows.iter().find(|r| r.buyout_count > 0).unwrap();
        assert_eq!(buyout.buyout_count, 3);
        assert_eq!(buyout.buyout_sum, 1500.0);

        let order = rows.iter().find(|r| r.order_count > 0).unwrap();
        assert_eq!(order.order_count, 5); // заказано всё
    }

    #[test]
    fn ym_returned_units_stay_in_buyout() {
        // Возврат после получения не уменьшает выкуп — движение возврата придёт из a016.
        let doc = ym_order(
            "DELIVERED",
            4.0,
            vec![("RETURNED", 1.0, "2026-04-20T10:00:00Z")],
        );
        let rows = from_ym_order(&doc, "reg-ym-4");
        let buyout = rows.iter().find(|r| r.buyout_count > 0).unwrap();
        assert_eq!(buyout.buyout_count, 4);
        assert!(rows.iter().all(|r| r.cancel_count == 0));
    }

    fn ym_return(return_type: &str) -> YmReturn {
        use contracts::domain::a016_ym_returns::aggregate::{
            YmReturn, YmReturnHeader, YmReturnLine, YmReturnSourceMeta, YmReturnState,
        };

        YmReturn::new_for_insert(
            "YM-RET-1".to_string(),
            "YM возврат".to_string(),
            YmReturnHeader {
                return_id: 501,
                order_id: 1001,
                connection_id: "conn-ym".to_string(),
                organization_id: "org-1".to_string(),
                marketplace_id: "mp-ym".to_string(),
                campaign_id: "camp-1".to_string(),
                return_type: return_type.to_string(),
                amount: Some(500.0),
                currency: Some("RUR".to_string()),
            },
            vec![YmReturnLine {
                item_id: 1,
                shop_sku: "SKU-1".to_string(),
                offer_id: "OFF-1".to_string(),
                name: "Товар".to_string(),
                count: 1,
                price: Some(500.0),
                return_reason: None,
                decisions: vec![],
                photos: vec![],
            }],
            YmReturnState {
                refund_status: "REFUNDED".to_string(),
                created_at_source: Some(utc("2026-04-20T10:00:00Z")),
                updated_at_source: Some(utc("2026-04-22T10:00:00Z")),
                refund_date: Some(utc("2026-04-25T10:00:00Z")),
            },
            YmReturnSourceMeta {
                raw_payload_ref: "raw-ret-1".to_string(),
                fetched_at: utc("2026-04-26T00:00:00Z"),
                document_version: 1,
            },
            true,
        )
    }

    #[test]
    fn ym_return_maps_to_return_movement_in_order_cohort() {
        let doc = ym_return("RETURN");
        let mut refs = std::collections::HashMap::new();
        refs.insert(
            "SKU-1".to_string(),
            (Some("mp-ym-1".to_string()), Some("nom-ym-1".to_string())),
        );
        let rows = from_ym_return(&doc, "reg-ret-1", Some("2026-04-01".to_string()), &refs);
        assert_eq!(rows.len(), 1);
        let r = &rows[0];
        assert_eq!(r.return_count, 1);
        assert_eq!(r.return_sum, 500.0);
        assert_eq!(r.cohort_date, "2026-04-01"); // когорта заказа
        assert_eq!(r.event_date, "2026-04-25"); // дата возврата денег
        assert_eq!(r.registrator_type, REG_A016);
        assert_eq!(r.marketplace_product_ref.as_deref(), Some("mp-ym-1"));
    }

    #[test]
    fn ym_unredeemed_is_not_projected_as_return() {
        // Невыкуп приходит из a013 как отказ; проведение его же из a016 задвоило бы отказы.
        let doc = ym_return("UNREDEEMED");
        let refs = std::collections::HashMap::new();
        let rows = from_ym_return(&doc, "reg-ret-2", None, &refs);
        assert!(rows.is_empty());
    }

    #[test]
    fn ym_shows_sales_maps_order_sum_and_keeps_delivered_in_source() {
        let header = YmShowsSalesDailyHeader {
            document_no: "YM-SF-2026-08-24".to_string(),
            document_date: "2026-08-24".to_string(),
            connection_id: "conn-ym".to_string(),
            organization_id: "org-1".to_string(),
            marketplace_id: "mp-ym".to_string(),
            campaign_id: Some("136982050".to_string()),
        };
        let line = YmShowsSalesDailyLine {
            offer_id: "SKU-1".to_string(),
            offer_name: "Товар".to_string(),
            marketplace_product_ref: Some("mp-1".to_string()),
            nomenclature_ref: Some("nom-1".to_string()),
            metrics: YmShowsSalesDailyMetrics {
                shows: Some(44),
                clicks: Some(5),
                to_cart: Some(1),
                order_items: Some(1),
                order_sum: Some(40826),
                delivered_count: Some(1),
                delivered_sum: Some(7054),
                ..Default::default()
            },
        };
        let doc = contracts::domain::a041_ym_shows_sales_daily::aggregate::YmShowsSalesDaily::new_for_insert(
            header,
            YmShowsSalesDailyMetrics::default(),
            vec![line],
            YmShowsSalesDailySourceMeta {
                source: "ym_shows_sales_report".to_string(),
                fetched_at: "2026-08-28T00:00:00Z".to_string(),
            },
        );

        let rows = from_ym_shows_sales_daily(&doc, "reg-a041-1");
        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(row.stage, "marketing");
        assert_eq!(row.funnel_order_count, 1);
        assert_eq!(row.funnel_order_sum, 40826.0);
        assert_eq!(row.total_impressions, Some(44));
        assert_eq!(row.open_count, 5);
        assert_eq!(row.cart_count, 1);
        // Доставки остаются в a041, в p916 marketing не проецируются.
        assert_eq!(row.buyout_count, 0);
        assert_eq!(row.buyout_sum, 0.0);
    }

    #[test]
    fn builder_ids_are_deterministic_across_runs() {
        let doc = wb_order(true, Some("2026-03-05T22:00:00Z"));
        let first = from_wb_orders(&doc, "reg-1");
        let second = from_wb_orders(&doc, "reg-1");
        let ids1: Vec<&str> = first.iter().map(|r| r.id.as_str()).collect();
        let ids2: Vec<&str> = second.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids1, ids2); // одинаковый вход → одинаковый id (идемпотентность upsert)
    }

    #[test]
    fn order_and_cancel_rows_have_distinct_ids_even_same_day() {
        // Заказ и отмена в один день: обе оси совпадают, различает только kind в id.
        let doc = wb_order(true, Some("2026-03-01T20:00:00Z"));
        let rows = from_wb_orders(&doc, "reg-1");
        let order = rows.iter().find(|r| r.order_count == 1).unwrap();
        let cancel = rows.iter().find(|r| r.cancel_count == 1).unwrap();
        assert_eq!(order.cohort_date, cancel.cohort_date);
        assert_eq!(order.event_date, cancel.event_date); // один день
        assert_ne!(order.id, cancel.id); // но id разные — нет схлопывания
    }
}
