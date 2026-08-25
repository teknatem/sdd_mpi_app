//! Вкладка «Проблемы» — единицы с нарушениями инвариантов §7.
//!
//! Отдельной вкладкой, а не фильтром на «Единицах», потому что это другой жанр:
//! там таблица для разглядывания, здесь список работ. Он и отсортирован иначе —
//! по числу нарушений, а не по идентификатору.

use leptos::prelude::*;

use crate::knowledge::view_model::InventoryVm;

#[component]
pub fn IssuesTab(vm: InventoryVm) -> impl IntoView {
    view! {
        {move || {
            let Some(data) = vm.data.get() else { return ().into_any() };
            let mut broken: Vec<_> = data
                .units
                .iter()
                .filter(|unit| !unit.issues.is_empty())
                .cloned()
                .collect();
            broken.sort_by(|a, b| b.issues.len().cmp(&a.issues.len()));

            if broken.is_empty() {
                return view! {
                    <div class="knowledge-inventory__empty">
                        "Нарушений инвариантов нет. Проверялись: наличие summary, минимальная \
                         длина тела, теги в словаре, якоря в реестре объектов, разрешимость \
                         ссылок related, имена инструментов у навыков."
                    </div>
                }.into_any();
            }

            view! {
                <div class="knowledge-inventory__issue-list">
                    {broken.into_iter().map(|unit| view! {
                        <div class="knowledge-inventory__issue">
                            <div class="knowledge-inventory__issue-head">
                                <span class="knowledge-inventory__id">{unit.unit_id.clone()}</span>
                                <span>{unit.title.clone()}</span>
                            </div>
                            <ul class="knowledge-inventory__named-list">
                                {unit.issues.iter().map(|issue| view! {
                                    <li>{issue.clone()}</li>
                                }).collect_view()}
                            </ul>
                        </div>
                    }).collect_view()}
                </div>
            }.into_any()
        }}
    }
}
