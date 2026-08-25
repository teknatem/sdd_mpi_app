//! Состояние страницы и команды.
//!
//! Фильтрация вся здесь, а не в разметке: осей девять, и раскиданная по
//! компонентам она превратилась бы в девять мест, где можно забыть новую.
//!
//! Ни одного кода классификатора страница не знает: и список осей, и подписи
//! значений приходят с бэкенда в `axes`/`facets`. Добавить ось — правка
//! `contracts::knowledge::classifiers`, здесь не меняется ничего.

use std::collections::BTreeMap;

use contracts::knowledge::{InventoryResponseDto, KnowledgeUnitDto};
use leptos::prelude::*;
use leptos::task::spawn_local;

use super::api;

/// Сколько строк показываем за раз. Четыреста единиц в один DOM класть незачем.
pub const PAGE_SIZE: usize = 50;

#[derive(Clone, Copy)]
pub struct InventoryVm {
    pub data: RwSignal<Option<InventoryResponseDto>>,
    pub loading: RwSignal<bool>,
    pub collecting: RwSignal<bool>,
    pub error: RwSignal<Option<String>>,
    pub notice: RwSignal<Option<String>>,
    pub tab: RwSignal<&'static str>,
    /// Выбранные значения по осям: ось → множество кодов. Пустое множество —
    /// ось не фильтрует.
    pub filters: RwSignal<BTreeMap<String, Vec<String>>>,
    pub search: RwSignal<String>,
    /// Показывать только единицы с нарушениями инвариантов.
    pub only_issues: RwSignal<bool>,
    pub page: RwSignal<usize>,
    pub selected: RwSignal<Option<String>>,
}

impl Default for InventoryVm {
    fn default() -> Self {
        Self::new()
    }
}

impl InventoryVm {
    pub fn new() -> Self {
        Self {
            data: RwSignal::new(None),
            loading: RwSignal::new(false),
            collecting: RwSignal::new(false),
            error: RwSignal::new(None),
            notice: RwSignal::new(None),
            tab: RwSignal::new("summary"),
            filters: RwSignal::new(BTreeMap::new()),
            search: RwSignal::new(String::new()),
            only_issues: RwSignal::new(false),
            page: RwSignal::new(0),
            selected: RwSignal::new(None),
        }
    }

    pub fn load(&self) {
        let vm = *self;
        spawn_local(async move {
            vm.loading.set(true);
            vm.error.set(None);
            match api::get_inventory().await {
                Ok(payload) => vm.data.set(Some(payload)),
                Err(message) => vm.error.set(Some(message)),
            }
            vm.loading.set(false);
        });
    }

    /// Пересобрать снимок и перечитать страницу.
    pub fn collect_now(&self) {
        let vm = *self;
        spawn_local(async move {
            vm.collecting.set(true);
            vm.error.set(None);
            vm.notice.set(None);
            match api::collect_now().await {
                Ok(report) => {
                    vm.notice.set(Some(format!(
                        "Снимок пересобран: {} единиц на {} поверхностях за {} мс.{}",
                        report.unit_count,
                        report.surface_count,
                        report.collect_ms,
                        if report.diagnostics.is_empty() {
                            String::new()
                        } else {
                            format!(" Замечания: {}", report.diagnostics.join("; "))
                        }
                    )));
                    vm.load();
                }
                Err(message) => vm.error.set(Some(message)),
            }
            vm.collecting.set(false);
        });
    }
}

impl InventoryVm {
    /// Переключить значение фильтра. Повторный клик снимает.
    pub fn toggle_filter(&self, axis: &str, code: &str) {
        self.filters.update(|filters| {
            let selected = filters.entry(axis.to_string()).or_default();
            match selected.iter().position(|item| item == code) {
                Some(index) => {
                    selected.remove(index);
                }
                None => selected.push(code.to_string()),
            }
        });
        // Фильтр меняет выборку целиком — оставаться на седьмой странице
        // прошлой выборки бессмысленно.
        self.page.set(0);
    }

    pub fn is_selected(&self, axis: &str, code: &str) -> bool {
        self.filters.with(|filters| {
            filters
                .get(axis)
                .is_some_and(|list| list.iter().any(|c| c == code))
        })
    }

    pub fn clear_filters(&self) {
        self.filters.update(BTreeMap::clear);
        self.search.set(String::new());
        self.only_issues.set(false);
        self.page.set(0);
    }

    pub fn active_filter_count(&self) -> usize {
        self.filters
            .with(|filters| filters.values().map(Vec::len).sum::<usize>())
            + usize::from(!self.search.get().trim().is_empty())
            + usize::from(self.only_issues.get())
    }

    /// Единицы после фильтров.
    ///
    /// Внутри оси значения складываются по ИЛИ, между осями — по И. Иначе выбор
    /// двух корпусов означал бы «принадлежит обоим», то есть пустоту.
    pub fn filtered_units(&self) -> Vec<KnowledgeUnitDto> {
        let Some(data) = self.data.get() else {
            return Vec::new();
        };
        let query = self.search.get().trim().to_lowercase();
        let only_issues = self.only_issues.get();

        self.filters.with(|filters| {
            data.units
                .iter()
                .filter(|unit| {
                    if only_issues && unit.issues.is_empty() {
                        return false;
                    }
                    if !query.is_empty()
                        && !unit.unit_id.to_lowercase().contains(&query)
                        && !unit.title.to_lowercase().contains(&query)
                        && !unit.subtitle.to_lowercase().contains(&query)
                    {
                        return false;
                    }
                    filters.iter().all(|(axis, selected)| {
                        selected.is_empty()
                            || selected.iter().any(|code| axis_value(unit, axis) == *code)
                    })
                })
                .cloned()
                .collect()
        })
    }
}

/// Значение оси у единицы. Единственное место, где ось сопоставляется с полем —
/// добавление оси правится здесь и только здесь.
fn axis_value(unit: &KnowledgeUnitDto, axis: &str) -> String {
    match axis {
        "family" => unit.family.as_str().to_string(),
        "origin" => unit.origin.as_str().to_string(),
        "storage_form" => unit.storage_form.as_str().to_string(),
        "editor" => unit.editor.as_str().to_string(),
        "reachability" => unit.reachability.as_str().to_string(),
        "lifecycle" => unit.lifecycle.as_str().to_string(),
        "scope" => unit.scope.as_str().to_string(),
        "channel" => unit.channel.as_str().to_string(),
        "code_role" => unit
            .code_role
            .map(|role| role.as_str().to_string())
            .unwrap_or_default(),
        // Поверхность осью не является, но фильтруется так же — с вкладки
        // «Поверхности» кликом по строке.
        "surface" => unit.surface_id.clone(),
        _ => String::new(),
    }
}
