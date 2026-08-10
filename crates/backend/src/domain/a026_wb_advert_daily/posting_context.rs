//! Контекст проведения a026: переиспользуемые между документами справочники.
//!
//! Проведение одного документа само по себе дёшево, но массовое (импорт за
//! период, перепроведение u508) упиралось в N+1: на каждую строку каждого
//! документа шёл запрос к a015 (`list_for_advert_attribution`, ~136 мс — фильтры
//! `substr(document_date,…)` и `json_extract(line_json,…)` не покрыты индексом)
//! плюс два точечных запроса к a007. За 12 месяцев это десятки тысяч запросов.
//!
//! Контекст существует в двух режимах:
//! - [`AdvertPostingContext::lazy`] — как раньше: запрос по мере надобности,
//!   результат мемоизируется. Режим одиночного проведения (UI, u508).
//! - [`AdvertPostingContext::prefetched`] — заказы a015, справочник a007 и
//!   накопленная атрибуция p913 читаются один раз на кабинет+период.
//!
//! **Порядок важен.** `build_linked_orders` сортирует кандидатов по уже
//! начисленной атрибуции из *других* документов, поэтому результат зависит от
//! порядка проведения. В предзагруженном режиме снимок p913 берётся один раз, а
//! вклад каждого проведённого документа доливается в память
//! ([`AdvertPostingContext::record_posted_reserve`]) — так пачка ведёт себя
//! ровно как последовательное проведение по одному.

use anyhow::Result;
use contracts::domain::a015_wb_orders::aggregate::WbOrders;
use contracts::domain::common::AggregateId;
use std::collections::HashMap;
use uuid::Uuid;

use crate::domain::a015_wb_orders;

/// Ключ группы заказов: (дата документа `YYYY-MM-DD`, nm_id).
type OrderGroupKey = (String, i64);

pub struct AdvertPostingContext {
    connection_id: String,
    /// Заказы a015 за период, сгруппированные по (дата, nm_id).
    /// `None` — ленивый режим (запрос на каждую пару).
    orders: Option<HashMap<OrderGroupKey, Vec<WbOrders>>>,
    /// nm_id → `nomenclature_ref` из a007. В предзагруженном режиме карта
    /// авторитетна: промах означает «в a007 такого sku нет», а не «ещё не читали».
    nomenclature_refs: HashMap<i64, Option<String>>,
    /// nm_id → id элемента a007 (мемоизация `find_or_create_for_advert`).
    product_refs: HashMap<i64, String>,
    /// `order_key` → накопленная reserve-сумма p913 по уже учтённым документам.
    /// `None` — ленивый режим.
    reserve_by_order_key: Option<HashMap<String, f64>>,
    prefetched: bool,
}

impl AdvertPostingContext {
    /// Контекст одиночного проведения: ничего не предзагружает, кэширует по ходу.
    pub fn lazy(connection_id: String) -> Self {
        Self {
            connection_id,
            orders: None,
            nomenclature_refs: HashMap::new(),
            product_refs: HashMap::new(),
            reserve_by_order_key: None,
            prefetched: false,
        }
    }

    /// Контекст массового проведения по кабинету за период.
    ///
    /// Снимок p913 обязан сниматься ПОСЛЕ удаления проекций за период
    /// (`replace_for_period`), иначе в основу попадут строки документов,
    /// которые сейчас же будут перезаписаны.
    pub async fn prefetched(connection_id: &str, date_from: &str, date_to: &str) -> Result<Self> {
        let started_at = std::time::Instant::now();

        let orders = a015_wb_orders::repository::list_for_advert_attribution_range(
            connection_id,
            date_from,
            date_to,
        )
        .await?;

        let products =
            crate::domain::a007_marketplace_product::repository::list_by_connection(connection_id)
                .await?;
        // Дубли a007 по одному sku в кабинете встречаются (авто-созданная запись
        // рядом с «богатой»), а точечный `get_by_connection_and_sku` берёт из них
        // произвольную. Здесь выбор детерминирован: приоритет у записи с
        // привязкой к номенклатуре, при равенстве — меньший id. Побочно это
        // связывает `nomenclature_ref` и `marketplace_product_ref` с ОДНОЙ
        // записью a007 (точечные резолверы могли взять разные).
        let mut chosen: HashMap<i64, (Option<String>, String)> =
            HashMap::with_capacity(products.len());
        for product in products {
            let Ok(nm_id) = product.marketplace_sku.trim().parse::<i64>() else {
                continue; // не-числовой sku в WB-кабинете: не наш случай
            };
            let nomenclature_ref = product
                .nomenclature_ref
                .as_ref()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty());
            let product_ref = product.base.id.as_string();

            // Ключ «чем меньше, тем лучше»: сначала записи с номенклатурой,
            // затем меньший id.
            let candidate_rank = (nomenclature_ref.is_none(), product_ref.as_str());
            let is_better = match chosen.get(&nm_id) {
                Some((current_nomenclature, current_product_ref)) => {
                    candidate_rank < (current_nomenclature.is_none(), current_product_ref.as_str())
                }
                None => true,
            };
            if is_better {
                chosen.insert(nm_id, (nomenclature_ref, product_ref));
            }
        }

        let mut nomenclature_refs: HashMap<i64, Option<String>> =
            HashMap::with_capacity(chosen.len());
        let mut product_refs: HashMap<i64, String> = HashMap::with_capacity(chosen.len());
        for (nm_id, (nomenclature_ref, product_ref)) in chosen {
            nomenclature_refs.insert(nm_id, nomenclature_ref);
            product_refs.insert(nm_id, product_ref);
        }

        let reserve_by_order_key =
            crate::projections::p913_wb_advert_order_attr::repository::sum_reserve_all_by_order_key()
                .await?;

        let order_groups = orders.len();
        let mut context = Self {
            connection_id: connection_id.to_string(),
            orders: Some(orders),
            nomenclature_refs,
            product_refs,
            reserve_by_order_key: Some(reserve_by_order_key),
            prefetched: true,
        };
        context.fill_missing_order_prices().await;

        tracing::info!(
            "a026 posting context prefetched: connection={}, period={}..{}, order_groups={}, products={}, reserve_keys={}, elapsed_ms={}",
            connection_id,
            date_from,
            date_to,
            order_groups,
            context.product_refs.len(),
            context
                .reserve_by_order_key
                .as_ref()
                .map(HashMap::len)
                .unwrap_or_default(),
            started_at.elapsed().as_millis()
        );

        Ok(context)
    }

    /// Дочитывает цену из marketplace-пейлоада там, где её нет на строке.
    ///
    /// В пер-документном режиме это делалось при каждом обращении к заказу;
    /// здесь — один раз на заказ, что эквивалентно (операция идемпотентна) и
    /// дешевле, когда заказ участвует сразу в нескольких кампаниях.
    async fn fill_missing_order_prices(&mut self) {
        let Some(orders) = self.orders.as_mut() else {
            return;
        };
        for group in orders.values_mut() {
            for order in group.iter_mut() {
                a015_wb_orders::service::fill_line_price_from_marketplace_raw(order).await;
            }
        }
    }

    /// Заказы-кандидаты для привязки рекламных расходов по (дата, nm_id).
    pub async fn orders_for(&mut self, document_date: &str, nm_id: i64) -> Result<Vec<WbOrders>> {
        if let Some(orders) = self.orders.as_ref() {
            return Ok(orders
                .get(&(document_date.to_string(), nm_id))
                .cloned()
                .unwrap_or_default());
        }

        let mut raw = a015_wb_orders::repository::list_for_advert_attribution(
            nm_id,
            &self.connection_id,
            document_date,
        )
        .await?;
        for order in raw.iter_mut() {
            a015_wb_orders::service::fill_line_price_from_marketplace_raw(order).await;
        }
        Ok(raw)
    }

    /// `nomenclature_ref` (a004) по nm_id.
    pub async fn nomenclature_ref(&mut self, nm_id: i64) -> Result<Option<String>> {
        if let Some(cached) = self.nomenclature_refs.get(&nm_id) {
            return Ok(cached.clone());
        }
        if self.prefetched {
            // Справочник кабинета прочитан целиком — промах означает отсутствие sku.
            return Ok(None);
        }

        let resolved =
            crate::domain::a007_marketplace_product::service::resolve_wb_nomenclature_ref(
                &self.connection_id,
                nm_id,
                None,
            )
            .await?;
        self.nomenclature_refs.insert(nm_id, resolved.clone());
        Ok(resolved)
    }

    /// id элемента a007 по nm_id; при отсутствии — создаёт элемент.
    pub async fn product_ref(
        &mut self,
        nm_id: i64,
        nm_name: &str,
        marketplace_ref: &str,
        document_no: &str,
        document_date: &str,
        document_id: Uuid,
    ) -> Result<String> {
        if let Some(cached) = self.product_refs.get(&nm_id) {
            return Ok(cached.clone());
        }

        let product_ref =
            crate::domain::a007_marketplace_product::service::find_or_create_for_advert(
                crate::domain::a007_marketplace_product::service::AdvertProductParams {
                    connection_mp_ref: self.connection_id.clone(),
                    marketplace_ref: marketplace_ref.to_string(),
                    nm_id,
                    nm_name: nm_name.to_string(),
                    document_no: document_no.to_string(),
                    document_id: document_id.to_string(),
                    document_date: document_date.to_string(),
                },
            )
            .await?;

        self.product_refs.insert(nm_id, product_ref.clone());
        // Свежесозданный элемент a007 идёт без привязки к номенклатуре.
        self.nomenclature_refs.entry(nm_id).or_insert(None);
        Ok(product_ref)
    }

    /// Накопленная атрибуция по заказам, исключая текущий документ.
    pub async fn reserve_sums(
        &mut self,
        order_keys: &[String],
        exclude_registrator_ref: &str,
    ) -> Result<HashMap<String, f64>> {
        if let Some(reserve) = self.reserve_by_order_key.as_ref() {
            // Вклад текущего документа в снимок ещё не долит (см. record_posted_reserve),
            // поэтому исключать его отдельно не нужно.
            return Ok(order_keys
                .iter()
                .filter_map(|key| reserve.get(key).map(|sum| (key.clone(), *sum)))
                .collect());
        }

        crate::projections::p913_wb_advert_order_attr::repository::sum_reserve_by_order_keys(
            order_keys,
            Some(exclude_registrator_ref),
        )
        .await
    }

    /// Долить вклад успешно проведённого документа в снимок атрибуции.
    /// Вызывать только после коммита транзакции — иначе повтор после ошибки
    /// SQLite-блокировки учёл бы документ дважды.
    pub fn record_posted_reserve(
        &mut self,
        entries: &[crate::projections::p913_wb_advert_order_attr::repository::Model],
    ) {
        let Some(reserve) = self.reserve_by_order_key.as_mut() else {
            return;
        };
        for entry in entries {
            if entry.order_key.is_empty() {
                continue;
            }
            *reserve.entry(entry.order_key.clone()).or_insert(0.0) += entry.amount;
        }
    }
}
