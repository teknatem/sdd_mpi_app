//! Вкладка «Единицы» — таблица всего, из чего система извлекает знание.
//!
//! Фильтры строятся из `facets`, приходящих с бэкенда: ни одного кода
//! классификатора здесь не написано. Появится десятая ось — она появится в
//! фильтрах сама, без правки этого файла.
//!
//! Счётчик рядом со значением — не украшение: он показывает, что выборка будет
//! непустой, ещё до клика, и сразу отвечает на вопрос «а такое вообще есть».

use leptos::prelude::*;

use crate::knowledge::view_model::{InventoryVm, PAGE_SIZE};

#[component]
pub fn UnitsTab(vm: InventoryVm) -> impl IntoView {
    let units = Memo::new(move |_| vm.filtered_units());
    let total = Memo::new(move |_| units.get().len());
    let page_count = Memo::new(move |_| total.get().div_ceil(PAGE_SIZE).max(1));

    view! {
        <div class="knowledge-inventory__filters">
            <div class="knowledge-inventory__filters-row">
                <input
                    class="knowledge-inventory__search"
                    type="search"
                    placeholder="Поиск по идентификатору, названию, пояснению"
                    prop:value=move || vm.search.get()
                    on:input=move |ev| {
                        vm.search.set(event_target_value(&ev));
                        vm.page.set(0);
                    }
                />
                <label class="knowledge-inventory__checkbox">
                    <input
                        type="checkbox"
                        prop:checked=move || vm.only_issues.get()
                        on:change=move |_| {
                            vm.only_issues.update(|value| *value = !*value);
                            vm.page.set(0);
                        }
                    />
                    "Только с нарушениями"
                </label>
                <Show when=move || { vm.active_filter_count() > 0 }>
                    <button class="button button--ghost" on:click=move |_| vm.clear_filters()>
                        "Сбросить (" {move || vm.active_filter_count()} ")"
                    </button>
                </Show>
            </div>

            {move || {
                let Some(data) = vm.data.get() else { return ().into_any() };
                data.facets.iter().map(|facet| {
                    let axis = facet.axis.clone();
                    view! {
                        <div class="knowledge-inventory__facet">
                            <span class="knowledge-inventory__facet-label">{facet.label.clone()}</span>
                            <div class="knowledge-inventory__chips">
                                {facet.values.iter().map(|value| {
                                    let axis = axis.clone();
                                    let code = value.code.clone();
                                    let code_for_class = code.clone();
                                    let axis_for_class = axis.clone();
                                    let count = value.count;
                                    view! {
                                        <button
                                            class=move || if vm.is_selected(&axis_for_class, &code_for_class) {
                                                "knowledge-inventory__chip knowledge-inventory__chip--on"
                                            } else if count == 0 {
                                                "knowledge-inventory__chip knowledge-inventory__chip--empty"
                                            } else {
                                                "knowledge-inventory__chip"
                                            }
                                            on:click=move |_| vm.toggle_filter(&axis, &code)
                                        >
                                            {value.label.clone()}
                                            <span class="knowledge-inventory__chip-count">{count}</span>
                                        </button>
                                    }
                                }).collect_view()}
                            </div>
                        </div>
                    }
                }).collect_view().into_any()
            }}
        </div>

        <div class="knowledge-inventory__toolbar">
            <span>
                "Показано " {move || {
                    let shown = units.get().len().min(PAGE_SIZE);
                    format!("{shown} из {}", total.get())
                }}
            </span>
            <div class="knowledge-inventory__pager">
                <button
                    class="button button--ghost"
                    disabled=move || vm.page.get() == 0
                    on:click=move |_| vm.page.update(|p| *p = p.saturating_sub(1))
                >"←"</button>
                <span>{move || format!("{} / {}", vm.page.get() + 1, page_count.get())}</span>
                <button
                    class="button button--ghost"
                    disabled=move || vm.page.get() + 1 >= page_count.get()
                    on:click=move |_| vm.page.update(|p| *p += 1)
                >"→"</button>
            </div>
        </div>

        <div class="knowledge-inventory__table-wrap">
            <table class="knowledge-inventory__table">
                <thead>
                    <tr>
                        <th>"Идентификатор"</th>
                        <th>"Название"</th>
                        <th>"Поверхность"</th>
                        <th>"Происхождение"</th>
                        <th>"Область"</th>
                        <th>"Достижимость"</th>
                        <th>"Цикл"</th>
                        <th class="knowledge-inventory__num">"Токены"</th>
                        <th class="knowledge-inventory__num">"Чтений"</th>
                        <th>"Замечания"</th>
                    </tr>
                </thead>
                <tbody>
                    {move || {
                        let page = vm.page.get();
                        units.get()
                            .into_iter()
                            .skip(page * PAGE_SIZE)
                            .take(PAGE_SIZE)
                            .map(|unit| {
                                let id = unit.unit_id.clone();
                                let selected = vm.selected.get() == Some(id.clone());
                                let issues = unit.issues.clone();
                                view! {
                                    <tr
                                        class=if selected { "knowledge-inventory__row--selected" } else { "" }
                                        on:click=move |_| vm.selected.set(Some(id.clone()))
                                    >
                                        <td class="knowledge-inventory__id">{unit.unit_id.clone()}</td>
                                        <td>
                                            <div>{unit.title.clone()}</div>
                                            <Show when={
                                                let sub = unit.subtitle.clone();
                                                move || !sub.is_empty()
                                            }>
                                                <div class="knowledge-inventory__sub">
                                                    {unit.subtitle.clone()}
                                                </div>
                                            </Show>
                                        </td>
                                        <td>{unit.surface_id.clone()}</td>
                                        <td>{unit.origin.label()}</td>
                                        <td>{unit.scope.label()}</td>
                                        <td>
                                            <span class=reachability_class(unit.reachability)>
                                                {unit.reachability.label()}
                                            </span>
                                        </td>
                                        <td>{unit.lifecycle.label()}</td>
                                        <td class="knowledge-inventory__num">
                                            {unit.tokens.map(|t| t.to_string()).unwrap_or_else(|| "—".into())}
                                        </td>
                                        <td class="knowledge-inventory__num">{unit.read_hits}</td>
                                        <td class="knowledge-inventory__issues">
                                            {if issues.is_empty() {
                                                String::new()
                                            } else {
                                                issues.join("; ")
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

        {move || {
            let Some(id) = vm.selected.get() else { return ().into_any() };
            let Some(unit) = units.get().into_iter().find(|u| u.unit_id == id) else {
                return ().into_any();
            };
            view! { <UnitDetail unit=unit /> }.into_any()
        }}

        <Show when=move || total.get() == 0 && !vm.loading.get()>
            <div class="knowledge-inventory__empty">
                "Под фильтры не попало ничего. Пустая выборка — тоже ответ: значит, \
                 такого сочетания в системе нет."
            </div>
        </Show>
    }
}

/// Недостижимость — единственное значение оси, которое красится: это дефект,
/// а не свойство. Остальные уровни равноправны и цвета не заслуживают.
fn reachability_class(value: contracts::knowledge::Reachability) -> &'static str {
    match value {
        contracts::knowledge::Reachability::Unreachable => {
            "knowledge-inventory__pill knowledge-inventory__pill--bad"
        }
        _ => "knowledge-inventory__pill",
    }
}

/// Полная карточка выбранной единицы.
///
/// Таблица показывает десять полей из двадцати — остальные нужны реже, но
/// нужны: чем перечисляется, кто правит, какая форма хранения, что за теги и
/// какие именно нарушения. Здесь они все.
#[component]
fn UnitDetail(unit: contracts::knowledge::KnowledgeUnitDto) -> impl IntoView {
    let rows: Vec<(&'static str, String)> = vec![
        ("Идентификатор", unit.unit_id.clone()),
        ("Поверхность", unit.surface_id.clone()),
        ("Семейство", unit.family.label().to_string()),
        ("Происхождение", unit.origin.label().to_string()),
        ("Форма хранения", unit.storage_form.label().to_string()),
        ("Кто правит", unit.editor.label().to_string()),
        ("Достижимость чатом", unit.reachability.label().to_string()),
        ("Жизненный цикл", unit.lifecycle.label().to_string()),
        ("Область", unit.scope.label().to_string()),
        ("Канал раскрытия", unit.channel.label().to_string()),
        (
            "Роль кода",
            unit.code_role
                .map(|role| role.label().to_string())
                .unwrap_or_else(|| "— код ни при чём".into()),
        ),
        (
            "Источник",
            unit.source_ref.clone().unwrap_or_else(|| "—".into()),
        ),
        (
            "Размер",
            unit.bytes
                .map(|b| format!("{b} байт"))
                .unwrap_or_else(|| "— не измеряется".into()),
        ),
        (
            "Токены",
            unit.tokens
                .map(|t| t.to_string())
                .unwrap_or_else(|| "— цена только у конкретного ответа".into()),
        ),
        (
            "Обращения",
            format!(
                "поиск {} · чтение {} · цитирование {}",
                unit.search_hits, unit.read_hits, unit.cited_hits
            ),
        ),
        (
            "Обновлено",
            unit.updated.clone().unwrap_or_else(|| "—".into()),
        ),
        (
            "Срок годности израсходован",
            unit.staleness_pct
                .map(|p| format!("{p} %"))
                .unwrap_or_else(|| "— неприменимо".into()),
        ),
        (
            "Теги",
            if unit.tags.is_empty() {
                "—".into()
            } else {
                unit.tags.join(", ")
            },
        ),
    ];

    view! {
        <div class="knowledge-inventory__detail">
            <div class="knowledge-inventory__block-title">{unit.title.clone()}</div>
            {(!unit.subtitle.is_empty()).then(|| view! {
                <div class="knowledge-inventory__sub">{unit.subtitle.clone()}</div>
            })}
            <dl class="knowledge-inventory__dl">
                {rows.into_iter().map(|(label, value)| view! {
                    <div class="knowledge-inventory__dl-row">
                        <dt>{label}</dt>
                        <dd>{value}</dd>
                    </div>
                }).collect_view()}
            </dl>
            {(!unit.issues.is_empty()).then(|| view! {
                <ul class="knowledge-inventory__named-list">
                    {unit.issues.iter().map(|issue| view! {
                        <li>{issue.clone()}</li>
                    }).collect_view()}
                </ul>
            })}
        </div>
    }
}
