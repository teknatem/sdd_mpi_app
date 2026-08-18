//! DataView — Источники данных
//!
//! Реестр именованных DataView: каждый view в своём подкаталоге dvNNN/.
//! На входе — ViewContext, на выходе — IndicatorValue (scalar) или DrilldownResponse.
//!
//! Кеш: `compute_scalar` сохраняет результаты в глобальном TTL-кеше (30 сек).
//! Несколько индикаторов дашборда с одинаковым DataView + контекстом не делают
//! повторных запросов к БД.

pub mod dv001_revenue;
pub mod dv002_wb_advert_by_items;
pub mod dv003_mp_order_line_turnovers;
pub mod dv004_general_ledger_turnovers;
pub mod dv005_gl_account_view_total;
pub mod dv006_indicator_ratio_percent;
pub mod dv007_gl_turnover_ratio_percent;
pub mod dv008_wb_sales_funnel;
pub mod filters;

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::Result;
use contracts::shared::analytics::IndicatorValue;
use contracts::shared::data_view::{DataViewMeta, FilterDef, ViewContext};
use contracts::shared::drilldown::{
    DrilldownCapabilitiesResponse, DrilldownDimensionCapability, DrilldownResponse,
};

// ---------------------------------------------------------------------------
// Global scalar result cache
// ---------------------------------------------------------------------------

const CACHE_TTL: Duration = Duration::from_secs(30);

struct CacheEntry {
    value: IndicatorValue,
    created_at: Instant,
}

struct InFlightEntry {
    result: Mutex<Option<std::result::Result<IndicatorValue, String>>>,
    notify: tokio::sync::Notify,
}

static SCALAR_CACHE: Mutex<Option<HashMap<String, CacheEntry>>> = Mutex::new(None);
static SCALAR_IN_FLIGHT: Mutex<Option<HashMap<String, Arc<InFlightEntry>>>> = Mutex::new(None);

fn cache_key(view_id: &str, ctx: &ViewContext) -> String {
    let refs = {
        let mut r = ctx.connection_mp_refs.clone();
        r.sort();
        r.join(",")
    };
    let params = {
        let mut pairs: Vec<(String, String)> = ctx
            .params
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        pairs.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
        pairs
            .into_iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join("&")
    };
    format!(
        "{view_id}|{}|{}|{}|{}|{refs}|{params}",
        ctx.date_from,
        ctx.date_to,
        ctx.period2_from.as_deref().unwrap_or(""),
        ctx.period2_to.as_deref().unwrap_or(""),
    )
}

fn cache_get(key: &str) -> Option<IndicatorValue> {
    let mut guard = SCALAR_CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let map = guard.get_or_insert_with(HashMap::new);
    if let Some(entry) = map.get(key) {
        if entry.created_at.elapsed() < CACHE_TTL {
            return Some(entry.value.clone());
        }
        map.remove(key);
    }
    None
}

fn cache_put(key: String, value: IndicatorValue) {
    let mut guard = SCALAR_CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let map = guard.get_or_insert_with(HashMap::new);
    // Evict expired entries periodically (when map grows).
    if map.len() > 256 {
        map.retain(|_, e| e.created_at.elapsed() < CACHE_TTL);
    }
    map.insert(
        key,
        CacheEntry {
            value,
            created_at: Instant::now(),
        },
    );
}

fn in_flight_get_or_insert(key: &str) -> (Arc<InFlightEntry>, bool) {
    let mut guard = SCALAR_IN_FLIGHT.lock().unwrap_or_else(|e| e.into_inner());
    let map = guard.get_or_insert_with(HashMap::new);
    if let Some(entry) = map.get(key) {
        return (entry.clone(), false);
    }

    let entry = Arc::new(InFlightEntry {
        result: Mutex::new(None),
        notify: tokio::sync::Notify::new(),
    });
    map.insert(key.to_string(), entry.clone());
    (entry, true)
}

fn in_flight_finish(
    key: &str,
    entry: &Arc<InFlightEntry>,
    result: std::result::Result<IndicatorValue, String>,
) {
    {
        let mut guard = entry.result.lock().unwrap_or_else(|e| e.into_inner());
        *guard = Some(result);
    }

    {
        let mut guard = SCALAR_IN_FLIGHT.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(map) = guard.as_mut() {
            map.remove(key);
        }
    }

    entry.notify.notify_waiters();
}

async fn wait_for_in_flight(entry: Arc<InFlightEntry>) -> Result<IndicatorValue> {
    loop {
        let notified = entry.notify.notified();
        if let Some(result) = entry
            .result
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
        {
            return result.map_err(anyhow::Error::msg);
        }
        notified.await;
    }
}

type ScalarFn =
    fn(&ViewContext) -> Pin<Box<dyn Future<Output = Result<IndicatorValue>> + Send + '_>>;

type DrillFn = for<'a> fn(
    &'a ViewContext,
    &'a str,
    &'a [String],
) -> Pin<Box<dyn Future<Output = Result<DrilldownResponse>> + Send + 'a>>;

type CapabilityFn = for<'a> fn(
    &'a ViewContext,
) -> Pin<
    Box<dyn Future<Output = Result<DrilldownCapabilitiesResponse>> + Send + 'a>,
>;

struct ViewEntry {
    scalar: ScalarFn,
    drilldown: DrillFn,
    capabilities: CapabilityFn,
    meta: DataViewMeta,
}

/// Реестр DataView: отображает view_id → функции вычисления + метаданные.
pub struct DataViewRegistry {
    views: HashMap<String, ViewEntry>,
}

impl DataViewRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            views: HashMap::new(),
        };

        registry.register(
            dv001_revenue::meta(),
            |ctx| Box::pin(dv001_revenue::compute_scalar(ctx)),
            |ctx, g, ids| Box::pin(dv001_revenue::compute_drilldown_multi(ctx, g, ids)),
            |ctx| Box::pin(Self::static_capabilities(dv001_revenue::meta(), ctx)),
        );
        registry.register(
            dv002_wb_advert_by_items::meta(),
            |ctx| Box::pin(dv002_wb_advert_by_items::compute_scalar(ctx)),
            |ctx, g, ids| {
                Box::pin(dv002_wb_advert_by_items::compute_drilldown_multi(
                    ctx, g, ids,
                ))
            },
            |ctx| {
                Box::pin(Self::static_capabilities(
                    dv002_wb_advert_by_items::meta(),
                    ctx,
                ))
            },
        );
        registry.register(
            dv003_mp_order_line_turnovers::meta(),
            |ctx| Box::pin(dv003_mp_order_line_turnovers::compute_scalar(ctx)),
            |ctx, g, ids| {
                Box::pin(dv003_mp_order_line_turnovers::compute_drilldown_multi(
                    ctx, g, ids,
                ))
            },
            |ctx| {
                Box::pin(Self::static_capabilities(
                    dv003_mp_order_line_turnovers::meta(),
                    ctx,
                ))
            },
        );
        registry.register(
            dv004_general_ledger_turnovers::meta(),
            |ctx| Box::pin(dv004_general_ledger_turnovers::compute_scalar(ctx)),
            |ctx, g, ids| {
                Box::pin(dv004_general_ledger_turnovers::compute_drilldown_multi(
                    ctx, g, ids,
                ))
            },
            |ctx| Box::pin(dv004_general_ledger_turnovers::compute_drilldown_capabilities(ctx)),
        );
        registry.register(
            dv005_gl_account_view_total::meta(),
            |ctx| Box::pin(dv005_gl_account_view_total::compute_scalar(ctx)),
            |ctx, g, ids| {
                Box::pin(dv005_gl_account_view_total::compute_drilldown_multi(
                    ctx, g, ids,
                ))
            },
            |ctx| {
                Box::pin(Self::static_capabilities(
                    dv005_gl_account_view_total::meta(),
                    ctx,
                ))
            },
        );
        registry.register(
            dv006_indicator_ratio_percent::meta(),
            |ctx| Box::pin(dv006_indicator_ratio_percent::compute_scalar(ctx)),
            |ctx, g, ids| {
                Box::pin(dv006_indicator_ratio_percent::compute_drilldown_multi(
                    ctx, g, ids,
                ))
            },
            |ctx| {
                Box::pin(Self::static_capabilities(
                    dv006_indicator_ratio_percent::meta(),
                    ctx,
                ))
            },
        );
        registry.register(
            dv007_gl_turnover_ratio_percent::meta(),
            |ctx| Box::pin(dv007_gl_turnover_ratio_percent::compute_scalar(ctx)),
            |ctx, g, ids| {
                Box::pin(dv007_gl_turnover_ratio_percent::compute_drilldown_multi(
                    ctx, g, ids,
                ))
            },
            |ctx| {
                Box::pin(Self::static_capabilities(
                    dv007_gl_turnover_ratio_percent::meta(),
                    ctx,
                ))
            },
        );
        registry.register(
            dv008_wb_sales_funnel::meta(),
            |ctx| Box::pin(dv008_wb_sales_funnel::compute_scalar(ctx)),
            |ctx, g, ids| Box::pin(dv008_wb_sales_funnel::compute_drilldown_multi(ctx, g, ids)),
            |ctx| {
                Box::pin(Self::static_capabilities(
                    dv008_wb_sales_funnel::meta(),
                    ctx,
                ))
            },
        );

        registry
    }

    fn register(
        &mut self,
        meta: DataViewMeta,
        scalar: ScalarFn,
        drilldown: DrillFn,
        capabilities: CapabilityFn,
    ) {
        let id = meta.id.clone();
        self.views.insert(
            id,
            ViewEntry {
                scalar,
                drilldown,
                capabilities,
                meta,
            },
        );
    }

    async fn static_capabilities(
        meta: DataViewMeta,
        _ctx: &ViewContext,
    ) -> Result<DrilldownCapabilitiesResponse> {
        Ok(DrilldownCapabilitiesResponse {
            safe_dimensions: meta
                .available_dimensions
                .into_iter()
                .map(|dimension| DrilldownDimensionCapability {
                    id: dimension.id,
                    label: dimension.label,
                    mode: "safe".to_string(),
                    coverage_pct: Some(100.0),
                    supported_turnover_codes: vec![],
                    missing_turnover_codes: vec![],
                })
                .collect(),
            partial_dimensions: vec![],
        })
    }

    /// Вычислить скалярное значение индикатора через DataView.
    ///
    /// Результат кешируется на [`CACHE_TTL`] по ключу `(view_id, context)`.
    /// Повторные вызовы с теми же параметрами (например, из нескольких индикаторов
    /// одного дашборда) не делают дополнительных запросов к БД.
    pub async fn compute_scalar(&self, view_id: &str, ctx: &ViewContext) -> Result<IndicatorValue> {
        let key = cache_key(view_id, ctx);

        if let Some(cached) = cache_get(&key) {
            tracing::debug!("DataView cache hit: {key}");
            return Ok(cached);
        }

        let (in_flight, is_leader) = in_flight_get_or_insert(&key);
        if !is_leader {
            tracing::debug!("DataView cache wait: {key}");
            return wait_for_in_flight(in_flight).await;
        }

        let entry = self
            .views
            .get(view_id)
            .ok_or_else(|| anyhow::anyhow!("DataView not found: {}", view_id))?;
        let result = (entry.scalar)(ctx).await;
        match result {
            Ok(value) => {
                cache_put(key.clone(), value.clone());
                in_flight_finish(&key, &in_flight, Ok(value.clone()));
                Ok(value)
            }
            Err(err) => {
                let message = err.to_string();
                in_flight_finish(&key, &in_flight, Err(message.clone()));
                Err(anyhow::Error::msg(message))
            }
        }
    }

    /// Вычислить drilldown через DataView.
    ///
    /// `metric_ids` — список запрошенных метрик. Пустой список = single-metric
    /// режим (backward compat, метрика берётся из ctx.params["metric"]).
    pub async fn compute_drilldown(
        &self,
        view_id: &str,
        ctx: &ViewContext,
        group_by: &str,
        metric_ids: &[String],
    ) -> Result<DrilldownResponse> {
        let entry = self
            .views
            .get(view_id)
            .ok_or_else(|| anyhow::anyhow!("DataView not found: {}", view_id))?;
        (entry.drilldown)(ctx, group_by, metric_ids).await
    }

    pub async fn compute_drilldown_capabilities(
        &self,
        view_id: &str,
        ctx: &ViewContext,
    ) -> Result<DrilldownCapabilitiesResponse> {
        let entry = self
            .views
            .get(view_id)
            .ok_or_else(|| anyhow::anyhow!("DataView not found: {}", view_id))?;
        (entry.capabilities)(ctx).await
    }

    /// Список метаданных всех зарегистрированных DataView.
    pub fn list_meta(&self) -> Vec<&DataViewMeta> {
        let mut list: Vec<&DataViewMeta> = self.views.values().map(|e| &e.meta).collect();
        list.sort_by(|a, b| a.id.cmp(&b.id));
        list
    }

    /// Метаданные конкретного DataView по id.
    pub fn get_meta(&self, id: &str) -> Option<&DataViewMeta> {
        self.views.get(id).map(|e| &e.meta)
    }

    /// Проверить, зарегистрирован ли view.
    pub fn has_view(&self, view_id: &str) -> bool {
        self.views.contains_key(view_id)
    }

    /// Резолвить фильтры конкретного DataView из глобального реестра.
    ///
    /// Берёт FilterRef[] из метаданных view, находит соответствующие FilterDef
    /// в глобальном реестре и возвращает их в порядке `order`.
    /// Применяет `label_override` если задан в FilterRef.
    pub fn resolve_filters(&self, view_id: &str) -> Vec<FilterDef> {
        let Some(meta) = self.get_meta(view_id) else {
            return vec![];
        };

        let registry = filters::global_filter_registry();
        let mut refs = meta.filters.clone();
        refs.sort_by_key(|r| r.order);

        refs.into_iter()
            .filter_map(|r| {
                let mut def = registry.get(&r.filter_id)?.clone();
                if let Some(label) = r.label_override {
                    def.label = label;
                }
                Some(def)
            })
            .collect()
    }
}

impl Default for DataViewRegistry {
    fn default() -> Self {
        Self::new()
    }
}
