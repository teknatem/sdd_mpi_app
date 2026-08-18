use super::super::{api, view_model::NomenclatureDetailsVm};
use crate::domain::a004_nomenclature::ui::picker::{NomenclaturePicker, NomenclaturePickerItem};
use crate::layout::global_context::AppGlobalContext;
use crate::shared::components::card_animated::CardAnimated;
use crate::shared::modal_stack::ModalStackService;
use leptos::prelude::*;
use thaw::*;

fn format_qty(value: f64) -> String {
    if (value.fract()).abs() < f64::EPSILON {
        format!("{}", value as i64)
    } else {
        format!("{value:.3}")
    }
}

#[component]
pub fn GeneralTab(vm: NomenclatureDetailsVm) -> impl IntoView {
    let tabs_store =
        leptos::context::use_context::<AppGlobalContext>().expect("AppGlobalContext not found");
    let modal_stack =
        leptos::context::use_context::<ModalStackService>().expect("ModalStackService not found");

    let description = vm.description;
    let full_description = vm.full_description;
    let code = vm.code;
    let article = vm.article;
    let parent_id = vm.parent_id;
    let comment = vm.comment;
    let is_folder = vm.is_folder;
    let is_derivative = vm.is_derivative;
    let base_nomenclature_ref = vm.base_nomenclature_ref;
    let alternative_cost_source_ref = vm.alternative_cost_source_ref;
    let alternative_cost_source_name = vm.alternative_cost_source_name;
    let alternative_cost_source_article = vm.alternative_cost_source_article;
    let kit_variant_ref = vm.kit_variant_ref;
    let kit_variant_info = vm.kit_variant_info;
    let kit_components = vm.kit_components;
    let kit_components_loading = vm.kit_components_loading;

    let is_kit = Signal::derive({
        let vm = vm.clone();
        move || vm.is_assembly.get() || !vm.kit_variant_ref.get().trim().is_empty()
    });

    Effect::new({
        let vm = vm.clone();
        move || {
            let alternative_ref = vm.alternative_cost_source_ref.get();
            if alternative_ref.trim().is_empty() {
                vm.alternative_cost_source_name.set(String::new());
                vm.alternative_cost_source_article.set(String::new());
                return;
            }

            let vm = vm.clone();
            leptos::task::spawn_local(async move {
                match api::fetch_base_nomenclature_info(&alternative_ref).await {
                    Ok(info) => {
                        vm.alternative_cost_source_name.set(info.name);
                        vm.alternative_cost_source_article.set(info.article);
                    }
                    Err(_) => {
                        vm.alternative_cost_source_name
                            .set(format!("[{}]", alternative_ref));
                        vm.alternative_cost_source_article.set(String::new());
                    }
                }
            });
        }
    });

    let open_alternative_picker = {
        let modal_stack = modal_stack.clone();
        let vm = vm.clone();
        move |_| {
            let vm_for_selected = vm.clone();
            let vm_for_cancel = vm.clone();
            let initial_selected_id = {
                let current = vm.alternative_cost_source_ref.get_untracked();
                if current.trim().is_empty() {
                    None
                } else {
                    Some(current)
                }
            };

            modal_stack.push_with_frame(
                Some(
                    "max-width: min(1100px, 95vw); width: min(1100px, 95vw); height: 85vh; overflow: hidden;"
                        .to_string(),
                ),
                Some("a004-alternative-cost-source-picker".to_string()),
                move |handle| {
                    let on_selected = {
                        let vm = vm_for_selected.clone();
                        let handle = handle.clone();
                        move |item: Option<NomenclaturePickerItem>| {
                            if let Some(item) = item {
                                vm.alternative_cost_source_ref.set(item.id.clone());
                                vm.alternative_cost_source_name.set(item.description);
                                vm.alternative_cost_source_article.set(item.article);
                            }
                            handle.close();
                        }
                    };

                    let on_cancel = {
                        let _vm = vm_for_cancel.clone();
                        let handle = handle.clone();
                        move |_| {
                            handle.close();
                        }
                    };

                    view! {
                        <NomenclaturePicker
                            initial_selected_id=initial_selected_id.clone()
                            title="Выберите альтернативный источник стоимости"
                            subtitle="Эта номенклатура будет использована для поиска себестоимости в p912, если по основной позиции стоимость не найдена."
                            search_placeholder="Поиск по артикулу, коду или наименованию"
                            empty_state_text="Подходящие позиции не найдены"
                            on_selected=on_selected
                            on_cancel=on_cancel
                        />
                    }
                    .into_any()
                },
            );
        }
    };

    let clear_alternative_source = {
        let vm = vm.clone();
        move |_| {
            vm.alternative_cost_source_ref.set(String::new());
            vm.alternative_cost_source_name.set(String::new());
            vm.alternative_cost_source_article.set(String::new());
        }
    };

    view! {
        <>
            <CardAnimated delay_ms=0 nav_id="a004_nomenclature_details_general_main">
                <h4 class="details-section__title">"Основные поля"</h4>
                <div class="details-grid--3col">
                    <div class="form__group" style="grid-column: 1 / -1;">
                        <label class="form__label">"Наименование *"</label>
                        <Input value=description placeholder="Введите наименование" />
                    </div>

                    <div class="form__group" style="grid-column: 1 / -1;">
                        <label class="form__label">"Полное наименование"</label>
                        <Input value=full_description placeholder="Опционально" />
                    </div>

                    <div class="form__group">
                        <label class="form__label">"Код"</label>
                        <Input value=code placeholder="Опционально" />
                    </div>

                    <div class="form__group">
                        <label class="form__label">"Артикул"</label>
                        <Input value=article placeholder="Опционально" />
                    </div>

                    <div class="form__group">
                        <label class="form__label">"Родитель (UUID)"</label>
                        <Input value=parent_id placeholder="Опционально" />
                    </div>

                    <div class="form__group" style="grid-column: 1 / -1;">
                        <label class="form__label">"Комментарий"</label>
                        <Textarea value=comment placeholder="Опционально" attr:rows=3 />
                    </div>

                    <div class="details-flags" style="grid-column: 1 / 2;">
                        <Checkbox checked=is_folder label="Это папка" />
                    </div>

                    <div class="details-flags" style="grid-column: 2 / -1;">
                        <Checkbox checked=is_derivative attr:disabled=true label="Производная позиция" />
                    </div>

                    <div class="form__group" style="grid-column: 1 / -1;">
                        <label class="form__label">"Базовая номенклатура (UUID)"</label>
                        <Input value=base_nomenclature_ref disabled=true placeholder="Не задано" />
                    </div>

                    <div class="form__group" style="grid-column: 1 / -1;">
                        <label class="form__label">"Альтернативный источник стоимости"</label>
                        <div style="display: flex; align-items: center; gap: var(--spacing-sm); flex-wrap: wrap;">
                            <Button appearance=ButtonAppearance::Secondary on_click=open_alternative_picker>
                                "Выбрать"
                            </Button>
                            <Button
                                appearance=ButtonAppearance::Subtle
                                on_click=clear_alternative_source
                                disabled=move || alternative_cost_source_ref.get().trim().is_empty()
                            >
                                "Очистить"
                            </Button>
                        </div>

                        <div style="margin-top: var(--spacing-sm); min-height: 24px;">
                            <Show
                                when=move || !alternative_cost_source_ref.get().trim().is_empty()
                                fallback=move || view! {
                                    <span style="color: var(--color-text-tertiary);">"Не задан"</span>
                                }
                            >
                                <div style="display: flex; align-items: center; gap: var(--spacing-sm); flex-wrap: wrap;">
                                    <a
                                        href="#"
                                        style="color: var(--color-primary); text-decoration: underline; cursor: pointer; font-weight: 500;"
                                        on:click={
                                            let tabs_store = tabs_store.clone();
                                            move |ev: web_sys::MouseEvent| {
                                                ev.prevent_default();
                                                let current_ref = alternative_cost_source_ref.get_untracked();
                                                if current_ref.is_empty() {
                                                    return;
                                                }

                                                let article = alternative_cost_source_article.get_untracked();
                                                let name = alternative_cost_source_name.get_untracked();
                                                let title = if article.trim().is_empty() {
                                                    format!("Номенклатура {}", name)
                                                } else {
                                                    format!("Номенклатура {}", article)
                                                };
                                                tabs_store.open_tab(
                                                    &format!("a004_nomenclature_details_{}", current_ref),
                                                    &title,
                                                );
                                            }
                                        }
                                    >
                                        {move || {
                                            let article = alternative_cost_source_article.get();
                                            let name = alternative_cost_source_name.get();
                                            if !article.trim().is_empty() && !name.trim().is_empty() {
                                                format!("{} ({})", article, name)
                                            } else if !article.trim().is_empty() {
                                                article
                                            } else if !name.trim().is_empty() {
                                                name
                                            } else {
                                                alternative_cost_source_ref.get()
                                            }
                                        }}
                                    </a>
                                    <span style="font-size: var(--font-size-sm); color: var(--color-text-tertiary);">
                                        {move || alternative_cost_source_ref.get()}
                                    </span>
                                </div>
                            </Show>
                        </div>
                    </div>
                </div>
            </CardAnimated>

            <CardAnimated delay_ms=40 nav_id="a004_nomenclature_details_general_kit">
                <h4 class="details-section__title">"Комплект"</h4>

                <Show
                    when=move || is_kit.get()
                    fallback=move || view! {
                        <div style="color: var(--color-text-tertiary);">"Не комплект"</div>
                    }
                >
                    <div style="display: grid; gap: var(--spacing-md);">
                        <div class="form__group">
                            <label class="form__label">"Вариант комплектации"</label>
                            <Show
                                when=move || !kit_variant_ref.get().trim().is_empty()
                                fallback=move || view! {
                                    <div style="display: flex; align-items: center; gap: var(--spacing-sm); flex-wrap: wrap;">
                                        <Badge appearance=BadgeAppearance::Filled color=BadgeColor::Warning>
                                            "Связь не определена"
                                        </Badge>
                                        <span style="color: var(--color-text-tertiary);">
                                            "Для комплекта не найден актуальный вариант a022_kit_variant"
                                        </span>
                                    </div>
                                }
                            >
                                <div style="display: flex; align-items: center; gap: var(--spacing-sm); flex-wrap: wrap;">
                                    <a
                                        href="#"
                                        style="color: var(--color-primary); text-decoration: underline; cursor: pointer; font-weight: 500;"
                                        on:click={
                                            let tabs_store = tabs_store.clone();
                                            move |ev: web_sys::MouseEvent| {
                                                ev.prevent_default();
                                                let current_ref = kit_variant_ref.get_untracked();
                                                if current_ref.is_empty() {
                                                    return;
                                                }

                                                let title = kit_variant_info
                                                    .get_untracked()
                                                    .map(|info| {
                                                        if info.code.trim().is_empty() {
                                                            format!("Вариант комплектации {}", info.description)
                                                        } else {
                                                            format!("Вариант комплектации {}", info.code)
                                                        }
                                                    })
                                                    .unwrap_or_else(|| "Вариант комплектации".to_string());

                                                tabs_store.open_tab(
                                                    &format!("a022_kit_variant_details_{}", current_ref),
                                                    &title,
                                                );
                                            }
                                        }
                                    >
                                        {move || {
                                            kit_variant_info
                                                .get()
                                                .map(|info| {
                                                    if info.code.trim().is_empty() {
                                                        info.description
                                                    } else if info.description.trim().is_empty() {
                                                        info.code
                                                    } else {
                                                        format!("{} ({})", info.code, info.description)
                                                    }
                                                })
                                                .unwrap_or_else(|| kit_variant_ref.get())
                                        }}
                                    </a>
                                </div>
                            </Show>
                        </div>

                        <div class="form__group">
                            <label class="form__label">"Комплектующие"</label>
                            <Show
                                when=move || kit_components_loading.get()
                                fallback=move || view! {
                                    <Show
                                        when=move || !kit_components.get().is_empty()
                                        fallback=move || view! {
                                            <div style="color: var(--color-text-tertiary);">
                                                "Нет данных по составу"
                                            </div>
                                        }
                                    >
                                        <div style="display: grid; gap: var(--spacing-xs);">
                                            <For
                                                each=move || kit_components.get()
                                                key=|item| format!("{}_{}", item.nomenclature_ref, item.quantity)
                                                children={
                                                    let tabs_store = tabs_store.clone();
                                                    move |item| {
                                                        let title = if item.article.trim().is_empty() {
                                                            item.description.clone()
                                                        } else {
                                                            format!("{} ({})", item.article, item.description)
                                                        };

                                                        view! {
                                                            <div
                                                                style="display: grid; grid-template-columns: minmax(0, 1fr) auto; align-items: center; gap: var(--spacing-sm); padding: 8px var(--spacing-md); border: 1px solid var(--color-border-secondary); border-radius: var(--radius-md); background: var(--color-bg-secondary);"
                                                            >
                                                                <div style="display: flex; align-items: baseline; gap: var(--spacing-sm); min-width: 0; overflow: hidden;">
                                                                    <a
                                                                        href="#"
                                                                        style="color: var(--color-primary); text-decoration: underline; cursor: pointer; font-weight: 500;"
                                                                        on:click={
                                                                            let nomenclature_ref = item.nomenclature_ref.clone();
                                                                            let title = title.clone();
                                                                            let tabs_store = tabs_store.clone();
                                                                            move |ev: web_sys::MouseEvent| {
                                                                                ev.prevent_default();
                                                                                tabs_store.open_tab(
                                                                                    &format!("a004_nomenclature_details_{}", nomenclature_ref),
                                                                                    &title,
                                                                                );
                                                                            }
                                                                        }
                                                                    >
                                                                        {if item.description.trim().is_empty() {
                                                                            item.nomenclature_ref.clone()
                                                                        } else {
                                                                            item.description.clone()
                                                                        }}
                                                                    </a>
                                                                    <span style="font-size: var(--font-size-sm); color: var(--color-text-tertiary); white-space: nowrap; overflow: hidden; text-overflow: ellipsis;">
                                                                        {if item.article.trim().is_empty() {
                                                                            item.nomenclature_ref.clone()
                                                                        } else {
                                                                            format!("Артикул: {}", item.article)
                                                                        }}
                                                                    </span>
                                                                </div>
                                                                <div style="text-align: right; font-variant-numeric: tabular-nums; white-space: nowrap; font-weight: 600;">
                                                                    {format_qty(item.quantity)}
                                                                </div>
                                                            </div>
                                                        }
                                                    }
                                                }
                                            />
                                        </div>
                                    </Show>
                                }
                            >
                                <div style="display: flex; align-items: center; gap: var(--spacing-sm); color: var(--color-text-tertiary);">
                                    <Spinner size=SpinnerSize::Tiny />
                                    <span>"Загрузка состава..."</span>
                                </div>
                            </Show>
                        </div>
                    </div>
                </Show>
            </CardAnimated>
        </>
    }
}
