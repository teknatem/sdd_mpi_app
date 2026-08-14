//! Preview tab: field-mapping table + live iframe sandbox.

use super::super::view_model::BiIndicatorDetailsVm;
use crate::shared::bi_card::{available_designs, default_design_name};
use crate::shared::components::card_animated::CardAnimated;
use crate::shared::icons::icon;
use leptos::prelude::*;
use thaw::*;
use wasm_bindgen::JsCast;

#[component]
pub fn PreviewTab(vm: BiIndicatorDetailsVm) -> impl IntoView {
    let srcdoc = vm.build_preview_srcdoc();
    let test_result = vm.test_result;
    let style_sig = vm.view_spec_style_name;
    let custom_css_sig = vm.view_spec_custom_css;
    let design_options =
        Signal::derive(move || available_designs(!custom_css_sig.get().trim().is_empty()));

    Effect::new(move |_| {
        if style_sig.get() == "custom" && custom_css_sig.get().trim().is_empty() {
            style_sig.set(default_design_name().to_string());
        }
    });

    let vm_apply_all = vm.clone();
    let apply_all = move |_| vm_apply_all.apply_test_to_preview();

    let vm_fill = vm.clone();
    let fill_demo = move |_| {
        vm_fill.preview_title.set("Выручка за 30 дней".to_string());
        vm_fill.preview_value.set("2 480 000 ₽".to_string());
        vm_fill.preview_unit.set("RUB".to_string());
        vm_fill.preview_delta.set("+12.4%".to_string());
        vm_fill.preview_delta_dir.set("up".to_string());
        vm_fill.preview_status.set("ok".to_string());
        vm_fill.preview_chip.set("Факт".to_string());
        vm_fill
            .preview_meta_1
            .set("Период: февраль 2026".to_string());
        vm_fill
            .preview_meta_2
            .set("Обновлено: 5 минут назад".to_string());
        vm_fill.preview_graph_type.set(2);
        vm_fill.preview_progress.set(78);
        vm_fill.preview_hint.set("к плану: 78%".to_string());
        vm_fill.preview_footer_1.set("Источник: Sales".to_string());
        vm_fill.preview_footer_2.set("Кабинеты: 4".to_string());
        vm_fill
            .preview_spark_points
            .set("42,44,43,47,49,52,50,55,58,60".to_string());
        vm_fill
            .preview_hidden_fields
            .set(std::collections::HashSet::new());
    };

    view! {
        <div class="bi-preview">
            <div class="bi-preview__toolbar">
                <Button appearance=ButtonAppearance::Subtle on_click=fill_demo.clone()>
                    "Заполнить демо-данными"
                </Button>
                {move || {
                    if test_result.get().is_some() {
                        view! {
                            <Button appearance=ButtonAppearance::Subtle on_click=apply_all.clone()>
                                {icon("download")} " Применить данные из теста"
                            </Button>
                        }
                            .into_any()
                    } else {
                        view! { <span /> }.into_any()
                    }
                }}
            </div>

            <div class="bi-preview__split">
                <div class="bi-preview__split-table">
                    <CardAnimated delay_ms=0 nav_id="a024_bi_indicator_details_preview_fields">
                        <h4 class="details-section__title">
                            {icon("table")} " Поля шаблона"
                        </h4>
                        <p class="form__hint">
                            "Слева задаются значения для рендера. Поля со стрелкой можно быстро заполнить данными из результата теста."
                        </p>
                        <div class="param-table-wrapper">
                            <table class="table__data param-table">
                                <thead>
                                    <tr>
                                        <th class="param-table__col-vis"></th>
                                        <th class="param-table__col-id">"id"</th>
                                        <th class="param-table__col-name">"Имя"</th>
                                        <th class="param-table__col-value">"Значение"</th>
                                        <th class="param-table__col-source">"Источник"</th>
                                        <th class="param-table__col-data">"Данные"</th>
                                        <th class="param-table__col-action"></th>
                                    </tr>
                                </thead>
                                <tbody>
                                    <FieldRow vm=vm.clone() row_key="name" label="Название" source_hint="—" styles_classic=true styles_modern=true styles_custom=true />
                                    <FieldRow vm=vm.clone() row_key="value" label="Значение" source_hint="value" styles_classic=true styles_modern=true styles_custom=true />
                                    <FieldRow vm=vm.clone() row_key="unit" label="Единица" source_hint="—" styles_classic=true styles_modern=false styles_custom=true />
                                    <FieldRow vm=vm.clone() row_key="delta" label="Изменение Δ" source_hint="change_percent" styles_classic=true styles_modern=true styles_custom=true />
                                    <FieldRowSelect vm=vm.clone() row_key="delta_dir" label="Направление Δ" source_hint="sign(chg_pct)" styles_classic=true styles_modern=true styles_custom=true options=vec![("up","↑ Вверх"),("down","↓ Вниз"),("flat","→ Нейтр")] />
                                    <FieldRowSelect vm=vm.clone() row_key="status" label="Статус" source_hint="status" styles_classic=true styles_modern=true styles_custom=true options=vec![("ok","ok — зелёный"),("bad","bad — красный"),("warn","warn — жёлтый"),("neutral","neutral — серый")] />
                                    <FieldRow vm=vm.clone() row_key="chip" label="Категория" source_hint="—" styles_classic=true styles_modern=true styles_custom=true />
                                    <FieldRow vm=vm.clone() row_key="meta_1" label="Мета 1" source_hint="—" styles_classic=true styles_modern=true styles_custom=true />
                                    <FieldRow vm=vm.clone() row_key="meta_2" label="Мета 2" source_hint="—" styles_classic=true styles_modern=true styles_custom=true />
                                    <FieldRowGraphType vm=vm.clone() />
                                    <FieldRowSpark vm=vm.clone() />
                                    <FieldRowNumber vm=vm.clone() row_key="progress" label="Прогресс %" source_hint="—" styles_classic=false styles_modern=true styles_custom=true />
                                    <FieldRow vm=vm.clone() row_key="hint" label="Подсказка" source_hint="—" styles_classic=false styles_modern=true styles_custom=true />
                                    <FieldRow vm=vm.clone() row_key="footer_1" label="Футер левый" source_hint="—" styles_classic=false styles_modern=true styles_custom=true />
                                    <FieldRow vm=vm.clone() row_key="footer_2" label="Футер правый" source_hint="—" styles_classic=false styles_modern=true styles_custom=true />
                                </tbody>
                            </table>
                        </div>
                    </CardAnimated>
                </div>

                <div class="bi-preview__split-preview">
                    <CardAnimated delay_ms=40 nav_id="a024_bi_indicator_details_preview_render">
                        <div class="bi-preview__preview-header">
                            <h4 class="details-section__title">
                                {icon("eye")} " Предпросмотр"
                            </h4>
                            <div class="bi-preview__design-picker">
                                <span class="bi-preview__design-label">"Дизайн"</span>
                                <select
                                    class="form__select form__select--sm"
                                    prop:value=move || style_sig.get()
                                    on:change=move |ev| {
                                        let target = ev.target().unwrap();
                                        let sel: &web_sys::HtmlSelectElement = target.unchecked_ref();
                                        style_sig.set(sel.value());
                                    }
                                >
                                    {move || {
                                        design_options
                                            .get()
                                            .into_iter()
                                            .map(|entry| view! { <option value=entry.key>{entry.label}</option> })
                                            .collect_view()
                                    }}
                                </select>
                            </div>
                        </div>
                        <p class="form__hint">
                            "Справа отображается тот же шаблон, который будет показан на дашборде."
                        </p>
                        <div class="bi-preview__sandbox">
                            <div class="bi-preview__frame-wrapper">
                                <iframe
                                    class="bi-preview__iframe"
                                    sandbox="allow-same-origin"
                                    srcdoc=move || srcdoc.get()
                                ></iframe>
                            </div>
                        </div>
                    </CardAnimated>
                </div>
            </div>
        </div>
    }
}

// Resolve computed test-data preview for supported keys.
fn compute_data_for_key(key: &str, vm: &BiIndicatorDetailsVm) -> Signal<Option<String>> {
    let test_result = vm.test_result;
    let format_vm = vm.clone();
    let key = key.to_string();
    Signal::derive(move || {
        let result = test_result.get()?;
        match key.as_str() {
            "value" => Some(format_vm.format_value(result.value)),
            "delta" => result.change_percent.map(|p| {
                let sign = if p >= 0.0 { "+" } else { "" };
                format!("{sign}{p:.2}%")
            }),
            "delta_dir" => result.change_percent.map(|p| {
                if p > 0.0 {
                    "up".to_string()
                } else if p < 0.0 {
                    "down".to_string()
                } else {
                    "flat".to_string()
                }
            }),
            "status" => Some(result.status.clone()),
            _ => None,
        }
    })
}

/// Return preview signal for a field key.
fn signal_for_key(key: &str, vm: &BiIndicatorDetailsVm) -> RwSignal<String> {
    match key {
        "name" => vm.preview_title,
        "value" => vm.preview_value,
        "unit" => vm.preview_unit,
        "delta" => vm.preview_delta,
        "delta_dir" => vm.preview_delta_dir,
        "status" => vm.preview_status,
        "chip" => vm.preview_chip,
        "meta_1" => vm.preview_meta_1,
        "meta_2" => vm.preview_meta_2,
        "hint" => vm.preview_hint,
        "footer_1" => vm.preview_footer_1,
        "footer_2" => vm.preview_footer_2,
        "spark" => vm.preview_spark_points,
        _ => vm.preview_title,
    }
}

fn row_opacity(
    _style_sig: RwSignal<String>,
    _classic: bool,
    _modern: bool,
    _custom: bool,
) -> Signal<f32> {
    Signal::derive(move || 1.0)
}

fn vis_checkbox(vm: &BiIndicatorDetailsVm, row_key: &'static str) -> impl IntoView {
    let hidden_sig = vm.preview_hidden_fields;
    view! {
        <input
            type="checkbox"
            class="param-table__vis-check"
            title="Показывать поле"
            prop:checked=move || !hidden_sig.get().contains(row_key)
            on:change=move |ev| {
                let checked = ev
                    .target()
                    .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
                    .map(|i| i.checked())
                    .unwrap_or(true);
                hidden_sig.update(|h| {
                    if checked {
                        h.remove(row_key);
                    } else {
                        h.insert(row_key.to_string());
                    }
                });
            }
        />
    }
}

#[component]
fn FieldRow(
    vm: BiIndicatorDetailsVm,
    row_key: &'static str,
    label: &'static str,
    source_hint: &'static str,
    styles_classic: bool,
    styles_modern: bool,
    styles_custom: bool,
) -> impl IntoView {
    let style_sig = vm.view_spec_style_name;
    let opacity = row_opacity(style_sig, styles_classic, styles_modern, styles_custom);
    let sig = signal_for_key(row_key, &vm);
    let data_sig = compute_data_for_key(row_key, &vm);

    view! {
        <tr style:opacity=move || opacity.get().to_string()>
            <td class="param-table__cell param-table__cell--vis">{vis_checkbox(&vm, row_key)}</td>
            <td class="param-table__cell param-table__cell--id"><code>{row_key}</code></td>
            <td class="param-table__cell param-table__cell--name">
                <span class="param-table__label">{label}</span>
            </td>
            <td class="param-table__cell param-table__cell--value">
                <input
                    type="text"
                    class="form__input param-table__input"
                    prop:value=move || sig.get()
                    on:input=move |ev| {
                        let input: web_sys::HtmlInputElement = ev.target().unwrap().unchecked_into();
                        sig.set(input.value());
                    }
                />
            </td>
            <td class="param-table__cell param-table__cell--source">
                <span class="param-table__source">{source_hint}</span>
            </td>
            <td class="param-table__cell param-table__cell--data">
                {move || match data_sig.get() {
                    Some(d) => view! { <span class="param-table__data">{d}</span> }.into_any(),
                    None => view! { <span class="param-table__data param-table__data--empty">"—"</span> }.into_any(),
                }}
            </td>
            <td class="param-table__cell param-table__cell--action">
                {move || data_sig.get().map(|d| {
                    let d = d.clone();
                    view! {
                        <button
                            class="param-table__copy-btn"
                            title="Копировать данные → Значение"
                            on:click=move |_| sig.set(d.clone())
                            type="button"
                        >
                            "←"
                        </button>
                    }
                })}
            </td>
        </tr>
    }
}

#[component]
fn FieldRowSelect(
    vm: BiIndicatorDetailsVm,
    row_key: &'static str,
    label: &'static str,
    source_hint: &'static str,
    styles_classic: bool,
    styles_modern: bool,
    styles_custom: bool,
    options: Vec<(&'static str, &'static str)>,
) -> impl IntoView {
    let style_sig = vm.view_spec_style_name;
    let opacity = row_opacity(style_sig, styles_classic, styles_modern, styles_custom);
    let sig = signal_for_key(row_key, &vm);
    let data_sig = compute_data_for_key(row_key, &vm);

    view! {
        <tr style:opacity=move || opacity.get().to_string()>
            <td class="param-table__cell param-table__cell--vis">{vis_checkbox(&vm, row_key)}</td>
            <td class="param-table__cell param-table__cell--id"><code>{row_key}</code></td>
            <td class="param-table__cell param-table__cell--name">
                <span class="param-table__label">{label}</span>
            </td>
            <td class="param-table__cell param-table__cell--value">
                <select
                    class="form__select param-table__input"
                    on:change=move |ev| {
                        let target = ev.target().unwrap();
                        let sel: &web_sys::HtmlSelectElement = target.unchecked_ref();
                        sig.set(sel.value());
                    }
                >
                    {options.iter().map(|(val, text)| {
                        let val = *val;
                        let text = *text;
                        view! {
                            <option value=val selected=move || sig.get() == val>
                                {text}
                            </option>
                        }
                    }).collect_view()}
                </select>
            </td>
            <td class="param-table__cell param-table__cell--source">
                <span class="param-table__source">{source_hint}</span>
            </td>
            <td class="param-table__cell param-table__cell--data">
                {move || match data_sig.get() {
                    Some(d) => view! { <span class="param-table__data">{d}</span> }.into_any(),
                    None => view! { <span class="param-table__data param-table__data--empty">"—"</span> }.into_any(),
                }}
            </td>
            <td class="param-table__cell param-table__cell--action">
                {move || data_sig.get().map(|d| {
                    let d = d.clone();
                    view! {
                        <button
                            class="param-table__copy-btn"
                            title="Копировать данные → Значение"
                            on:click=move |_| sig.set(d.clone())
                            type="button"
                        >
                            "←"
                        </button>
                    }
                })}
            </td>
        </tr>
    }
}

#[component]
fn FieldRowGraphType(vm: BiIndicatorDetailsVm) -> impl IntoView {
    let graph_type_sig = vm.preview_graph_type;

    view! {
        <tr>
            <td class="param-table__cell param-table__cell--vis"></td>
            <td class="param-table__cell param-table__cell--id"><code>{"graph_type"}</code></td>
            <td class="param-table__cell param-table__cell--name">
                <span class="param-table__label">"Тип графика"</span>
            </td>
            <td class="param-table__cell param-table__cell--value">
                <select
                    class="form__select param-table__input"
                    on:change=move |ev| {
                        let target = ev.target().unwrap();
                        let sel: &web_sys::HtmlSelectElement = target.unchecked_ref();
                        graph_type_sig.set(sel.value().parse::<u8>().unwrap_or(0).min(2));
                    }
                >
                    <option value="0" selected=move || graph_type_sig.get() == 0>"0 - ничего"</option>
                    <option value="1" selected=move || graph_type_sig.get() == 1>"1 - прогресс"</option>
                    <option value="2" selected=move || graph_type_sig.get() == 2>"2 - спарклайн"</option>
                </select>
            </td>
            <td class="param-table__cell param-table__cell--source">
                <span class="param-table__source">"—"</span>
            </td>
            <td class="param-table__cell param-table__cell--data">
                <span class="param-table__data param-table__data--empty">"—"</span>
            </td>
            <td class="param-table__cell param-table__cell--action"></td>
        </tr>
    }
}

#[component]
fn FieldRowSpark(vm: BiIndicatorDetailsVm) -> impl IntoView {
    let style_sig = vm.view_spec_style_name;
    let opacity = row_opacity(style_sig, true, false, false);
    let sig = vm.preview_spark_points;

    view! {
        <tr style:opacity=move || opacity.get().to_string()>
            <td class="param-table__cell param-table__cell--vis">{vis_checkbox(&vm, "spark")}</td>
            <td class="param-table__cell param-table__cell--id"><code>{"spark"}</code></td>
            <td class="param-table__cell param-table__cell--name">
                <span class="param-table__label">"Спарклайн"</span>
            </td>
            <td class="param-table__cell param-table__cell--value">
                <input
                    type="text"
                    class="form__input param-table__input"
                    placeholder="10,20,15,30,25"
                    prop:value=move || sig.get()
                    on:input=move |ev| {
                        let input: web_sys::HtmlInputElement = ev.target().unwrap().unchecked_into();
                        sig.set(input.value());
                    }
                />
            </td>
            <td class="param-table__cell param-table__cell--source">
                <span class="param-table__source">"—"</span>
            </td>
            <td class="param-table__cell param-table__cell--data">
                <span class="param-table__data param-table__data--empty">"—"</span>
            </td>
            <td class="param-table__cell param-table__cell--action"></td>
        </tr>
    }
}

#[component]
fn FieldRowNumber(
    vm: BiIndicatorDetailsVm,
    row_key: &'static str,
    label: &'static str,
    source_hint: &'static str,
    styles_classic: bool,
    styles_modern: bool,
    styles_custom: bool,
) -> impl IntoView {
    let style_sig = vm.view_spec_style_name;
    let opacity = row_opacity(style_sig, styles_classic, styles_modern, styles_custom);
    let progress_sig = vm.preview_progress;

    view! {
        <tr style:opacity=move || opacity.get().to_string()>
            <td class="param-table__cell param-table__cell--vis">{vis_checkbox(&vm, row_key)}</td>
            <td class="param-table__cell param-table__cell--id"><code>{row_key}</code></td>
            <td class="param-table__cell param-table__cell--name">
                <span class="param-table__label">{label}</span>
            </td>
            <td class="param-table__cell param-table__cell--value">
                <input
                    type="number"
                    class="form__input param-table__input"
                    min="0"
                    max="100"
                    prop:value=move || progress_sig.get().to_string()
                    on:input=move |ev| {
                        let input: web_sys::HtmlInputElement = ev.target().unwrap().unchecked_into();
                        let val: u8 = input.value().parse().unwrap_or(0);
                        progress_sig.set(val.min(100));
                    }
                />
            </td>
            <td class="param-table__cell param-table__cell--source">
                <span class="param-table__source">{source_hint}</span>
            </td>
            <td class="param-table__cell param-table__cell--data">
                <span class="param-table__data param-table__data--empty">"—"</span>
            </td>
            <td class="param-table__cell param-table__cell--action"></td>
        </tr>
    }
}
