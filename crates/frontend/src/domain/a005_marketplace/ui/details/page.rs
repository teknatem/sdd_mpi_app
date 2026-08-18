use super::view_model::MarketplaceDetailsViewModel;
use crate::shared::icons::icon;
use contracts::enums::marketplace_type::MarketplaceType;
use leptos::prelude::*;
use std::rc::Rc;
use thaw::*;

#[component]
pub fn MarketplaceDetails(
    id: Option<String>,
    #[prop(optional)] readonly: bool,
    on_saved: Callback<()>,
    on_cancel: Callback<()>,
) -> impl IntoView {
    let vm = MarketplaceDetailsViewModel::new();
    vm.load_if_needed(id);

    // Individual RwSignals for Thaw form components
    let description = RwSignal::new(String::new());
    let url = RwSignal::new(String::new());
    let logo_path = RwSignal::new(String::new());
    let marketplace_type_code = RwSignal::new(String::new());
    let acquiring_fee_str = RwSignal::new(String::new());
    let comment = RwSignal::new(String::new());

    // Sync vm.form → individual signals (runs on load and when server data arrives)
    let vm_for_sync = vm.clone();
    Effect::new(move |_| {
        let form = vm_for_sync.form.get();
        description.set(form.description.clone());
        url.set(form.url.clone());
        logo_path.set(form.logo_path.clone().unwrap_or_default());
        marketplace_type_code.set(
            form.marketplace_type
                .map(|t| t.code().to_string())
                .unwrap_or_default(),
        );
        acquiring_fee_str.set(format!("{:.2}", form.acquiring_fee_pro));
        comment.set(form.comment.clone().unwrap_or_default());
    });

    // Inline validation (mirrors ViewModel logic) for button disabled state
    let is_valid = Signal::derive(move || {
        let desc = description.get();
        let u = url.get();
        !desc.trim().is_empty()
            && !u.trim().is_empty()
            && (u.starts_with("http://") || u.starts_with("https://"))
    });

    // Sync individual signals → vm.form before saving
    let vm_for_save = vm.clone();
    let sync_and_save = {
        let on_saved = on_saved.clone();
        move |_: leptos::ev::MouseEvent| {
            vm_for_save.form.update(|f| {
                f.description = description.get();
                f.url = url.get();
                let lp = logo_path.get();
                f.logo_path = if lp.is_empty() { None } else { Some(lp) };
                f.marketplace_type = {
                    let code = marketplace_type_code.get();
                    if code.is_empty() {
                        None
                    } else {
                        MarketplaceType::from_code(&code)
                    }
                };
                f.acquiring_fee_pro = acquiring_fee_str.get().parse().unwrap_or(0.0);
                let c = comment.get();
                f.comment = if c.is_empty() { None } else { Some(c) };
            });
            let on_saved_cb = on_saved.clone();
            let on_saved_rc: Rc<dyn Fn(())> = Rc::new(move |_| on_saved_cb.run(()));
            vm_for_save.save_command(on_saved_rc);
        }
    };

    let vm_for_title = vm.clone();
    let vm_for_error = vm.clone();

    view! {
        <div id="a005_marketplace--detail" data-page-category="legacy" class="details-container marketplace-details">
            <div class="modal-header">
                <h3 class="modal-title">
                    {move || if readonly {
                        "Маркетплейс"
                    } else if vm_for_title.is_edit_mode()() {
                        "Редактирование маркетплейса"
                    } else {
                        "Новый маркетплейс"
                    }}
                </h3>
                <div class="modal-header-actions">
                    <Show when=move || !readonly>
                        <Button
                            appearance=ButtonAppearance::Primary
                            on_click=sync_and_save.clone()
                            disabled=Signal::derive(move || !is_valid.get())
                        >
                            {icon("save")}
                            " Сохранить"
                        </Button>
                    </Show>
                    <Button
                        appearance=ButtonAppearance::Transparent
                        on_click=move |_| on_cancel.run(())
                    >
                        {icon("x")}
                        " Закрыть"
                    </Button>
                </div>
            </div>

            {move || vm_for_error.error.get().map(|e| view! {
                <div class="warning-box warning-box--error">
                    <span class="warning-box__icon">"⚠"</span>
                    <span class="warning-box__text">{e}</span>
                </div>
            })}

            <div class="modal-body" style="display:flex; flex-direction:column; gap:8px; padding: 16px">
                <div class="form__group">
                    <label class="form__label" for="mp-description">{"Наименование"}</label>
                    <Input
                        value=description
                        placeholder="Введите наименование маркетплейса"
                        disabled=readonly
                        attr:id="mp-description"
                    />
                </div>

                <div class="form__group">
                    <label class="form__label" for="mp-url">{"URL"}</label>
                    <Input
                        value=url
                        placeholder="https://example.com"
                        disabled=readonly
                        attr:id="mp-url"
                    />
                </div>

                <div class="form__group">
                    <label class="form__label" for="mp-type">{"Тип маркетплейса"}</label>
                    <Select value=marketplace_type_code attr:id="mp-type" disabled=readonly>
                        <option value="">{"-- Не выбрано --"}</option>
                        {MarketplaceType::all().into_iter().map(|mp_type| {
                            let code = mp_type.code();
                            let name = mp_type.display_name();
                            view! {
                                <option value={code}>{name}</option>
                            }
                        }).collect_view()}
                    </Select>
                </div>

                <div class="form__group">
                    <label class="form__label" for="mp-logo">{"Путь к логотипу"}</label>
                    <Input
                        value=logo_path
                        placeholder="/assets/images/logo.svg"
                        disabled=readonly
                        attr:id="mp-logo"
                    />
                </div>

                <div class="form__group">
                    <label class="form__label" for="mp-acquiring">{"Эквайринг, %"}</label>
                    <input
                        class="form__input"
                        type="number"
                        step="0.01"
                        min="0"
                        max="100"
                        id="mp-acquiring"
                        disabled=readonly
                        prop:value=move || acquiring_fee_str.get()
                        on:input=move |ev| acquiring_fee_str.set(event_target_value(&ev))
                        placeholder="0.00"
                    />
                </div>

                <div class="form__group">
                    <label class="form__label" for="mp-comment">{"Комментарий"}</label>
                    <Textarea
                        value=comment
                        placeholder="Введите дополнительную информацию (необязательно)"
                        disabled=readonly
                        resize=TextareaResize::Vertical
                        attr:id="mp-comment"
                        attr:rows="3"
                    />
                </div>
            </div>
        </div>
    }
}
