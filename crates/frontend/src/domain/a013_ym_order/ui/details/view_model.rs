//! ViewModel for YM Order details

use super::api::*;
use leptos::prelude::*;
use std::collections::HashMap;
use wasm_bindgen_futures::spawn_local;

#[derive(Clone)]
pub struct YmOrderDetailsVm {
    pub id: RwSignal<Option<String>>,
    pub order: RwSignal<Option<YmOrderDetailDto>>,

    pub raw_json: RwSignal<Option<String>>,
    pub raw_json_loaded: RwSignal<bool>,
    pub raw_json_loading: RwSignal<bool>,

    pub projections: RwSignal<Option<serde_json::Value>>,
    pub projections_loaded: RwSignal<bool>,
    pub projections_loading: RwSignal<bool>,

    pub payment_reports: RwSignal<Vec<YmPaymentReportLinkDto>>,
    pub payment_reports_loaded: RwSignal<bool>,
    pub payment_reports_loading: RwSignal<bool>,
    pub payment_reports_error: RwSignal<Option<String>>,

    pub nomenclatures_info: RwSignal<HashMap<String, NomenclatureInfo>>,
    pub marketplace_products_info: RwSignal<HashMap<String, MarketplaceProductInfo>>,

    /// Таймлайн событий заказа из p915 (вкладка «Общие»).
    pub order_events: RwSignal<Vec<OrderEventDto>>,
    /// Наименования связей (вместо uuid в блоке «Связи»).
    pub connection_name: RwSignal<Option<String>>,
    pub organization_name: RwSignal<Option<String>>,
    pub marketplace_name: RwSignal<Option<String>>,

    pub active_tab: RwSignal<&'static str>,
    pub loading: RwSignal<bool>,
    pub posting: RwSignal<bool>,
    pub error: RwSignal<Option<String>>,
}

impl YmOrderDetailsVm {
    pub fn new() -> Self {
        Self {
            id: RwSignal::new(None),
            order: RwSignal::new(None),

            raw_json: RwSignal::new(None),
            raw_json_loaded: RwSignal::new(false),
            raw_json_loading: RwSignal::new(false),

            projections: RwSignal::new(None),
            projections_loaded: RwSignal::new(false),
            projections_loading: RwSignal::new(false),

            payment_reports: RwSignal::new(Vec::new()),
            payment_reports_loaded: RwSignal::new(false),
            payment_reports_loading: RwSignal::new(false),
            payment_reports_error: RwSignal::new(None),

            nomenclatures_info: RwSignal::new(HashMap::new()),
            marketplace_products_info: RwSignal::new(HashMap::new()),

            order_events: RwSignal::new(Vec::new()),
            connection_name: RwSignal::new(None),
            organization_name: RwSignal::new(None),
            marketplace_name: RwSignal::new(None),

            active_tab: RwSignal::new("general"),
            loading: RwSignal::new(false),
            posting: RwSignal::new(false),
            error: RwSignal::new(None),
        }
    }

    pub fn is_posted(&self) -> Signal<bool> {
        let order = self.order;
        Signal::derive(move || order.get().map(|o| o.metadata.is_posted).unwrap_or(false))
    }

    pub fn document_no(&self) -> Signal<String> {
        let order = self.order;
        Signal::derive(move || {
            order
                .get()
                .map(|o| o.header.document_no.clone())
                .unwrap_or_default()
        })
    }

    pub fn projections_count(&self) -> Signal<usize> {
        let projections = self.projections;
        Signal::derive(move || {
            projections
                .get()
                .as_ref()
                .map(|p| {
                    let p900_len = p["p900_sales_register"]
                        .as_array()
                        .map(|a| a.len())
                        .unwrap_or(0);
                    let p904_len = p["p904_sales_data"]
                        .as_array()
                        .map(|a| a.len())
                        .unwrap_or(0);
                    p900_len + p904_len
                })
                .unwrap_or(0)
        })
    }

    pub fn set_tab(&self, tab: &'static str) {
        self.active_tab.set(tab);
    }

    pub fn load(&self, id: String) {
        let vm = self.clone();
        vm.id.set(Some(id.clone()));
        vm.loading.set(true);
        vm.error.set(None);

        spawn_local(async move {
            match fetch_by_id(&id).await {
                Ok(data) => {
                    vm.load_general_extras(&data.header);
                    vm.load_order_events(&data.header.document_no);
                    vm.order.set(Some(data.clone()));
                    vm.load_line_links(data.lines);
                    vm.load_projections();
                    // Грузим сразу, чтобы бейдж на ярлыке «Связи» показывал реальное
                    // количество строк с самого открытия, а не 0 до первого клика.
                    vm.load_payment_reports();
                    vm.loading.set(false);
                }
                Err(e) => {
                    vm.error.set(Some(e));
                    vm.loading.set(false);
                }
            }
        });
    }

    fn load_line_links(&self, lines: Vec<LineDto>) {
        for line in lines {
            let line_id = line.line_id.clone();

            if let Some(nom_ref) = line.nomenclature_ref {
                let nom_store = self.nomenclatures_info;
                let line_id_nom = line_id.clone();
                spawn_local(async move {
                    if let Ok(info) = fetch_nomenclature(&nom_ref).await {
                        nom_store.update(|map| {
                            map.insert(line_id_nom, info);
                        });
                    }
                });
            }

            if let Some(mp_ref) = line.marketplace_product_ref {
                let mp_store = self.marketplace_products_info;
                let line_id_mp = line_id;
                spawn_local(async move {
                    if let Ok(info) = fetch_marketplace_product(&mp_ref).await {
                        mp_store.update(|map| {
                            map.insert(line_id_mp, info);
                        });
                    }
                });
            }
        }
    }

    /// Загрузить наименования связей (подключение/организация/маркетплейс) по id.
    fn load_general_extras(&self, header: &HeaderDto) {
        let conn_id = header.connection_id.clone();
        let org_id = header.organization_id.clone();
        let mp_id = header.marketplace_id.clone();
        let conn_store = self.connection_name;
        let org_store = self.organization_name;
        let mp_store = self.marketplace_name;
        spawn_local(async move {
            conn_store.set(fetch_connection_name(&conn_id).await);
        });
        spawn_local(async move {
            org_store.set(fetch_organization_name(&org_id).await);
        });
        spawn_local(async move {
            mp_store.set(fetch_marketplace_name(&mp_id).await);
        });
    }

    /// Загрузить таймлайн событий заказа из p915 (отсортирован по дате).
    fn load_order_events(&self, document_no: &str) {
        let order_id = document_no.to_string();
        let store = self.order_events;
        spawn_local(async move {
            match fetch_order_events(&order_id).await {
                Ok(mut events) => {
                    events.sort_by(|a, b| a.event_date.cmp(&b.event_date));
                    store.set(events);
                }
                Err(e) => leptos::logging::log!("Failed to load order events: {}", e),
            }
        });
    }

    pub fn load_raw_json(&self) {
        // get_untracked: не создавать реактивную зависимость в вызывающем Effect,
        // иначе тумблер raw_json_loading перезапускал бы Effect бесконечно.
        if self.raw_json_loaded.get_untracked() || self.raw_json_loading.get_untracked() {
            return;
        }
        let Some(order) = self.order.get_untracked() else {
            return;
        };

        let raw_payload_ref = order.source_meta.raw_payload_ref.clone();
        let vm = self.clone();
        vm.raw_json_loading.set(true);

        spawn_local(async move {
            match fetch_raw_json(&raw_payload_ref).await {
                Ok(json) => {
                    vm.raw_json.set(Some(json));
                }
                Err(e) => leptos::logging::log!("Failed to load raw JSON: {}", e),
            }
            // Помечаем как «загружено» и при ошибке: raw-payload может отсутствовать
            // в БД (штатная ситуация) — не повторяем запрос и не крутим спиннер вечно.
            vm.raw_json_loaded.set(true);
            vm.raw_json_loading.set(false);
        });
    }

    pub fn load_projections(&self) {
        if self.projections_loaded.get_untracked() || self.projections_loading.get_untracked() {
            return;
        }
        let Some(id) = self.id.get_untracked() else {
            return;
        };

        let vm = self.clone();
        vm.projections_loading.set(true);
        spawn_local(async move {
            match fetch_projections(&id).await {
                Ok(p) => {
                    vm.projections.set(Some(p));
                    vm.projections_loaded.set(true);
                }
                Err(e) => leptos::logging::log!("Failed to load projections: {}", e),
            }
            vm.projections_loading.set(false);
        });
    }

    pub fn load_payment_reports(&self) {
        if self.payment_reports_loaded.get_untracked()
            || self.payment_reports_loading.get_untracked()
        {
            return;
        }
        let Some(order) = self.order.get_untracked() else {
            return;
        };

        // document_no is the YM integer order id stored as a string
        let order_id: i64 = match order.header.document_no.parse() {
            Ok(v) => v,
            Err(_) => {
                self.payment_reports_error.set(Some(format!(
                    "Не удалось разобрать номер заказа: {}",
                    order.header.document_no
                )));
                return;
            }
        };

        let vm = self.clone();
        vm.payment_reports_loading.set(true);
        vm.payment_reports_error.set(None);

        spawn_local(async move {
            match fetch_payment_reports_by_order(order_id).await {
                Ok(reports) => {
                    vm.payment_reports.set(reports);
                    vm.payment_reports_loaded.set(true);
                }
                Err(e) => {
                    leptos::logging::log!("Failed to load payment reports: {}", e);
                    vm.payment_reports_error.set(Some(e));
                }
            }
            vm.payment_reports_loading.set(false);
        });
    }

    pub fn payment_reports_count(&self) -> Signal<usize> {
        let payment_reports = self.payment_reports;
        Signal::derive(move || payment_reports.get().len())
    }

    pub fn post(&self) {
        let Some(id) = self.id.get() else {
            return;
        };
        let vm = self.clone();
        vm.posting.set(true);

        spawn_local(async move {
            if let Err(e) = post_document(&id).await {
                leptos::logging::log!("Failed to post document: {}", e);
            } else {
                vm.reload().await;
            }
            vm.posting.set(false);
        });
    }

    pub fn unpost(&self) {
        let Some(id) = self.id.get() else {
            return;
        };
        let vm = self.clone();
        vm.posting.set(true);

        spawn_local(async move {
            if let Err(e) = unpost_document(&id).await {
                leptos::logging::log!("Failed to unpost document: {}", e);
            } else {
                vm.reload().await;
            }
            vm.posting.set(false);
        });
    }

    async fn reload(&self) {
        let Some(id) = self.id.get() else {
            return;
        };

        if let Ok(data) = fetch_by_id(&id).await {
            self.nomenclatures_info.set(HashMap::new());
            self.marketplace_products_info.set(HashMap::new());
            self.load_general_extras(&data.header);
            self.load_order_events(&data.header.document_no);
            self.load_line_links(data.lines.clone());
            self.order.set(Some(data));
        }

        if self.projections_loaded.get() {
            if let Ok(p) = fetch_projections(&id).await {
                self.projections.set(Some(p));
            }
        }
    }
}

impl Default for YmOrderDetailsVm {
    fn default() -> Self {
        Self::new()
    }
}
