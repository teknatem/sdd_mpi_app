use super::view_model::OrganizationDetailsViewModel;
use crate::shared::icons::icon;
use leptos::prelude::*;
use std::rc::Rc;

#[component]
pub fn OrganizationDetails(
    id: Option<String>,
    on_saved: Rc<dyn Fn(())>,
    on_cancel: Rc<dyn Fn(())>,
) -> impl IntoView {
    let vm = OrganizationDetailsViewModel::new();
    vm.load_if_needed(id);

    // Clone vm for multiple closures
    let vm_clone = vm.clone();

    view! {
        <div>
            <div class="modal-header" style="display: flex; justify-content: space-between; align-items: center;">
                <h3 class="modal-title">
                    {
                        let vm = vm_clone.clone();
                        move || if vm.is_edit_mode()() { "Редактирование организации" } else { "Новая организация" }
                    }
                </h3>
                <div style="display: flex; gap: var(--spacing-sm);">
                    <button
                        class="button button--primary"
                        on:click={
                            let vm = vm_clone.clone();
                            let on_saved = on_saved.clone();
                            move |_| vm.save_command(on_saved.clone())
                        }
                        disabled={
                            let vm = vm_clone.clone();
                            move || !vm.is_form_valid()()
                        }
                    >
                        {icon("save")}
                        {
                            let vm = vm_clone.clone();
                            move || if vm.is_edit_mode()() { "Сохранить" } else { "Создать" }
                        }
                    </button>
                    <button
                        class="button button--secondary"
                        on:click=move |_| (on_cancel)(())
                    >
                        {icon("x")}
                        {"Закрыть"}
                    </button>
                </div>
            </div>

            <div class="modal-body" style="border: none;">
                {
                    let vm = vm_clone.clone();
                    move || vm.error.get().map(|e| view! {
                        <div class="warning-box" style="background: var(--color-error-50); border-color: var(--color-error-100); margin-bottom: var(--spacing-md);">
                            <span class="warning-box__icon" style="color: var(--color-error);">"⚠"</span>
                            <span class="warning-box__text" style="color: var(--color-error);">{e}</span>
                        </div>
                    })
                }

                <div class="form__group">
                    <label class="form__label" for="description">{"Наименование"}</label>
                    <input
                        class="form__input"
                        type="text"
                        id="description"
                        prop:value={
                            let vm = vm_clone.clone();
                            move || vm.form.get().description
                        }
                        on:input={
                            let vm = vm_clone.clone();
                            move |ev| {
                                vm.form.update(|f| f.description = event_target_value(&ev));
                            }
                        }
                        placeholder="Введите наименование организации"
                    />
                </div>

                <div class="form__group">
                    <label class="form__label" for="inn">{"ИНН"}</label>
                    <input
                        class="form__input"
                        type="text"
                        id="inn"
                        prop:value={
                            let vm = vm_clone.clone();
                            move || vm.form.get().inn
                        }
                        on:input={
                            let vm = vm_clone.clone();
                            move |ev| {
                                vm.form.update(|f| f.inn = event_target_value(&ev));
                            }
                        }
                        placeholder="10 или 12 цифр"
                        maxlength="12"
                    />
                </div>

                <div class="form__group">
                    <label class="form__label" for="kpp">{"КПП"}</label>
                    <input
                        class="form__input"
                        type="text"
                        id="kpp"
                        prop:value={
                            let vm = vm_clone.clone();
                            move || vm.form.get().kpp
                        }
                        on:input={
                            let vm = vm_clone.clone();
                            move |ev| {
                                vm.form.update(|f| f.kpp = event_target_value(&ev));
                            }
                        }
                        placeholder="9 цифр (необязательно для ИП)"
                        maxlength="9"
                    />
                </div>

                <div class="form__group">
                    <label class="form__label" for="entity_ref">{"Субъект учёта GL"}</label>
                    <select
                        class="form__input"
                        id="entity_ref"
                        prop:value={
                            let vm = vm_clone.clone();
                            move || vm.form.get().entity_ref.clone().unwrap_or_default()
                        }
                        on:change={
                            let vm = vm_clone.clone();
                            move |ev| {
                                let value = event_target_value(&ev);
                                vm.form.update(|f| {
                                    f.entity_ref = if value.is_empty() { None } else { Some(value) };
                                });
                            }
                        }
                    >
                        <option value="">{"— не задан —"}</option>
                        {contracts::general_ledger::GL_ENTITY_CLASSES
                            .iter()
                            .filter(|e| e.kind == "own")
                            .map(|e| {
                                let v = e.code.to_string();
                                let l = format!("{} — {}", e.code, e.name);
                                view! { <option value=v>{l}</option> }
                            })
                            .collect::<Vec<_>>()}
                    </select>
                </div>

                <div class="form__group">
                    <label class="form__label" for="comment">{"Комментарий"}</label>
                    <textarea
                        class="form__textarea"
                        id="comment"
                        prop:value={
                            let vm = vm_clone.clone();
                            move || vm.form.get().comment.clone().unwrap_or_default()
                        }
                        on:input={
                            let vm = vm_clone.clone();
                            move |ev| {
                                let value = event_target_value(&ev);
                                vm.form.update(|f| {
                                    f.comment = if value.is_empty() { None } else { Some(value) };
                                });
                            }
                        }
                        placeholder="Введите дополнительную информацию (необязательно)"
                        rows="3"
                    />
                </div>
            </div>
        </div>
    }
}
