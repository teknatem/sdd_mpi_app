//! Вкладка «Поверхности» — реестр §2 строками.
//!
//! Здесь видно то, чего не видно нигде больше: чем поверхность перечисляется,
//! какие инструменты её отдают и совпало ли заявленное с измеренным. Последняя
//! колонка — самая полезная: расхождение означает, что мы про систему думаем
//! не то, что в ней есть.

use leptos::prelude::*;

use crate::knowledge::view_model::InventoryVm;

#[component]
pub fn SurfacesTab(vm: InventoryVm) -> impl IntoView {
    view! {
        <div class="knowledge-inventory__table-wrap">
            <table class="knowledge-inventory__table">
                <thead>
                    <tr>
                        <th>"Поверхность"</th>
                        <th>"Семейство"</th>
                        <th class="knowledge-inventory__num">"Единиц"</th>
                        <th class="knowledge-inventory__num">"Токенов"</th>
                        <th>"Достижимость"</th>
                        <th>"Инструменты"</th>
                        <th>"Чем перечисляется"</th>
                    </tr>
                </thead>
                <tbody>
                    {move || {
                        let Some(data) = vm.data.get() else { return ().into_any() };
                        data.surfaces.clone().into_iter().map(|surface| {
                            let id = surface.surface_id.clone();
                            let mismatch =
                                surface.reachability_declared != surface.reachability_effective;
                            view! {
                                <tr on:click=move |_| {
                                    // Клик по поверхности — переход к её единицам:
                                    // самый частый следующий вопрос после «а что
                                    // это вообще такое».
                                    vm.clear_filters();
                                    vm.toggle_filter("surface", &id);
                                    vm.tab.set("units");
                                }>
                                    <td>
                                        <div>{surface.label}</div>
                                        <div class="knowledge-inventory__sub">{surface.note}</div>
                                    </td>
                                    <td>{surface.family.label()}</td>
                                    <td class="knowledge-inventory__num">{surface.unit_count}</td>
                                    <td class="knowledge-inventory__num">
                                        {surface.stored_tokens.map(|t| t.to_string()).unwrap_or_else(|| "—".into())}
                                    </td>
                                    <td>
                                        <span class=if mismatch {
                                            "knowledge-inventory__pill knowledge-inventory__pill--warn"
                                        } else {
                                            "knowledge-inventory__pill"
                                        }>
                                            {surface.reachability_effective.label()}
                                        </span>
                                        <Show when=move || mismatch>
                                            <div class="knowledge-inventory__sub">
                                                "заявлено: " {surface.reachability_declared.label()}
                                            </div>
                                        </Show>
                                    </td>
                                    <td class="knowledge-inventory__sub">
                                        {if surface.tools.is_empty() {
                                            "— ни одного".to_string()
                                        } else {
                                            surface.tools.join(", ")
                                        }}
                                    </td>
                                    <td class="knowledge-inventory__id">{surface.enumerated_by}</td>
                                </tr>
                            }
                        }).collect_view().into_any()
                    }}
                </tbody>
            </table>
        </div>
    }
}
