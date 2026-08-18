//! ViewModel for WB Product Snapshot details.

use super::api::*;
use crate::layout::global_context::AppGlobalContext;
use crate::shared::list_utils::sort_list;
use leptos::prelude::*;
use leptos::task::spawn_local;

#[derive(Clone)]
pub struct WbProductSnapshotDetailsVm {
    pub tabs: AppGlobalContext,

    pub id: RwSignal<Option<String>>,
    pub doc: RwSignal<Option<DetailsDto>>,

    pub active_tab: RwSignal<&'static str>,
    pub loading: RwSignal<bool>,
    pub error: RwSignal<Option<String>>,

    pub lines_sort_field: RwSignal<String>,
    pub lines_sort_ascending: RwSignal<bool>,

    // Динамика: выбранный товар и загруженная серия.
    pub selected_nm_id: RwSignal<Option<i64>>,
    pub series: RwSignal<Vec<SeriesPointDto>>,
    pub series_loading: RwSignal<bool>,
    pub series_from: RwSignal<String>,
    pub series_to: RwSignal<String>,

    // Воронка (a036): метрики nm_id → (переходы, в корзину, заказы) за N дней.
    pub funnel_days: RwSignal<i64>,
    pub funnel_metrics: RwSignal<std::collections::HashMap<i64, (i64, i64, i64)>>,
    pub funnel_loading: RwSignal<bool>,
    pub funnel_loaded: RwSignal<bool>,

    // Изменения рейтинга/оценки vs предыдущий снимок.
    pub changes: RwSignal<Vec<RatingChangeDto>>,
    pub changes_loading: RwSignal<bool>,
    pub changes_loaded: RwSignal<bool>,
    pub changes_has_previous: RwSignal<bool>,
    pub changes_prev_date: RwSignal<Option<String>>,
}

impl WbProductSnapshotDetailsVm {
    pub fn new(tabs: AppGlobalContext) -> Self {
        let today = chrono::Utc::now().date_naive();
        let month_ago = today - chrono::Duration::days(29);
        Self {
            tabs,
            id: RwSignal::new(None),
            doc: RwSignal::new(None),
            active_tab: RwSignal::new("general"),
            loading: RwSignal::new(true),
            error: RwSignal::new(None),
            lines_sort_field: RwSignal::new("title".to_string()),
            lines_sort_ascending: RwSignal::new(true),
            selected_nm_id: RwSignal::new(None),
            series: RwSignal::new(Vec::new()),
            series_loading: RwSignal::new(false),
            series_from: RwSignal::new(month_ago.format("%Y-%m-%d").to_string()),
            series_to: RwSignal::new(today.format("%Y-%m-%d").to_string()),
            funnel_days: RwSignal::new(7),
            funnel_metrics: RwSignal::new(std::collections::HashMap::new()),
            funnel_loading: RwSignal::new(false),
            funnel_loaded: RwSignal::new(false),
            changes: RwSignal::new(Vec::new()),
            changes_loading: RwSignal::new(false),
            changes_loaded: RwSignal::new(false),
            changes_has_previous: RwSignal::new(false),
            changes_prev_date: RwSignal::new(None),
        }
    }

    pub fn header_title(&self) -> Signal<String> {
        let doc = self.doc;
        Signal::derive(move || {
            doc.get()
                .map(|d| {
                    format!(
                        "Данные по товарам WB {} от {}",
                        d.document_no,
                        fmt_date(&d.document_date)
                    )
                })
                .unwrap_or_else(|| "Данные по товарам WB".to_string())
        })
    }

    pub fn tab_label(&self) -> Signal<String> {
        let doc = self.doc;
        Signal::derive(move || {
            doc.get()
                .map(|d| format!("Товары WB {}", d.document_date))
                .unwrap_or_else(|| "Товары WB".to_string())
        })
    }

    pub fn sorted_lines(&self) -> Signal<Vec<LineDto>> {
        let doc = self.doc;
        let field = self.lines_sort_field;
        let ascending = self.lines_sort_ascending;
        Signal::derive(move || {
            let mut lines = doc.get().map(|d| d.lines).unwrap_or_default();
            sort_list(&mut lines, &field.get(), ascending.get());
            lines
        })
    }

    pub fn set_tab(&self, tab: &'static str) {
        self.active_tab.set(tab);
    }

    pub fn toggle_lines_sort(&self, field: &'static str) {
        if self.lines_sort_field.get_untracked() == field {
            self.lines_sort_ascending.update(|value| *value = !*value);
        } else {
            self.lines_sort_field.set(field.to_string());
            self.lines_sort_ascending.set(true);
        }
    }

    pub fn load(&self, id: String) {
        let vm = self.clone();
        vm.id.set(Some(id.clone()));
        vm.loading.set(true);
        vm.error.set(None);

        spawn_local(async move {
            match fetch_by_id(&id).await {
                Ok(data) => {
                    let tab_id = id.clone();
                    let title = format!("Товары WB {}", data.document_date);
                    vm.tabs.update_tab_title(
                        &format!("a037_wb_product_snapshot_details_{tab_id}"),
                        &title,
                    );
                    // По умолчанию для динамики выбираем первый товар.
                    if vm.selected_nm_id.get_untracked().is_none() {
                        if let Some(first) = data.lines.first() {
                            vm.selected_nm_id.set(Some(first.nm_id));
                        }
                    }
                    vm.doc.set(Some(data));
                    vm.loading.set(false);
                }
                Err(e) => {
                    vm.error.set(Some(e));
                    vm.loading.set(false);
                }
            }
        });
    }

    /// Загрузка динамики выбранного товара за диапазон series_from..series_to.
    pub fn load_series(&self) {
        let Some(doc) = self.doc.get_untracked() else {
            return;
        };
        let Some(nm_id) = self.selected_nm_id.get_untracked() else {
            return;
        };
        let connection_id = doc.connection_id.clone();
        let date_from = self.series_from.get_untracked();
        let date_to = self.series_to.get_untracked();

        let vm = self.clone();
        vm.series_loading.set(true);
        spawn_local(async move {
            match fetch_series(&connection_id, nm_id, &date_from, &date_to).await {
                Ok(points) => vm.series.set(points),
                Err(e) => {
                    vm.error.set(Some(e));
                    vm.series.set(Vec::new());
                }
            }
            vm.series_loading.set(false);
        });
    }

    /// Загрузка метрик воронки (a036) за окно [дата_снимка − (N−1) … дата_снимка].
    pub fn load_funnel(&self) {
        let Some(doc) = self.doc.get_untracked() else {
            return;
        };
        let connection_id = doc.connection_id.clone();
        let date_to = doc.document_date.clone();
        let days = self.funnel_days.get_untracked().max(1);
        let date_from = chrono::NaiveDate::parse_from_str(&date_to, "%Y-%m-%d")
            .map(|d| {
                (d - chrono::Duration::days(days - 1))
                    .format("%Y-%m-%d")
                    .to_string()
            })
            .unwrap_or_else(|_| date_to.clone());

        let vm = self.clone();
        vm.funnel_loading.set(true);
        vm.funnel_loaded.set(true);
        spawn_local(async move {
            match fetch_product_metrics(&connection_id, &date_from, &date_to).await {
                Ok(rows) => {
                    let map: std::collections::HashMap<i64, (i64, i64, i64)> = rows
                        .into_iter()
                        .map(|r| (r.nm_id, (r.open_count, r.cart_count, r.order_count)))
                        .collect();
                    vm.funnel_metrics.set(map);
                }
                Err(e) => vm.error.set(Some(e)),
            }
            vm.funnel_loading.set(false);
        });
    }

    /// Автозагрузка воронки при первом открытии вкладки «Позиции».
    pub fn ensure_funnel(&self) {
        if !self.funnel_loaded.get_untracked() {
            self.load_funnel();
        }
    }

    /// Загрузка позиций с изменившимся рейтингом/оценкой vs прошлый снимок.
    pub fn load_changes(&self) {
        let Some(id) = self.id.get_untracked() else {
            return;
        };
        let vm = self.clone();
        vm.changes_loading.set(true);
        vm.changes_loaded.set(true);
        spawn_local(async move {
            match fetch_rating_changes(&id).await {
                Ok(resp) => {
                    vm.changes_has_previous.set(resp.has_previous);
                    vm.changes_prev_date.set(resp.prev_date);
                    vm.changes.set(resp.rows);
                }
                Err(e) => {
                    vm.error.set(Some(e));
                    vm.changes.set(Vec::new());
                }
            }
            vm.changes_loading.set(false);
        });
    }

    /// Автозагрузка вкладки «Изменения» при первом открытии.
    pub fn ensure_changes(&self) {
        if !self.changes_loaded.get_untracked() {
            self.load_changes();
        }
    }

    pub fn open_product(&self, product_ref: String) {
        if product_ref.is_empty() {
            return;
        }
        self.tabs.open_tab(
            &format!("a007_marketplace_product_details_{}", product_ref),
            "Товар маркетплейса",
        );
    }
}
