//! LLM Connection Details - View Component
//!
//! Full-page form (открывается вкладкой) для создания/редактирования подключения LLM.
//! Вся работа с моделями собрана в один блок «Модели»: явная кнопка загрузки каталога,
//! таблица с мини-фильтром и сортировкой, выбор разрешённых моделей (чекбокс) и основной
//! модели (звёздочка) прямо в строках.

use super::model::{fetch_connection, fetch_models_from_api, save_connection, test_connection};
use super::view_model::{model_provider, LlmConnectionDetailsVm, ModelSortCol};
use crate::shared::icons::icon;
use crate::shared::page_frame::PageFrame;
use crate::shared::page_standard::PAGE_CAT_DETAIL;
use leptos::prelude::*;
use thaw::*;

#[component]
#[allow(non_snake_case)]
pub fn LlmConnectionDetails(
    id: Option<String>,
    on_saved: Callback<()>,
    on_cancel: Callback<()>,
) -> impl IntoView {
    let vm = LlmConnectionDetailsVm::new();

    // Текущий id элемента. Инициализируется из props; после автосохранения черновика
    // (создание нового) сюда записывается свежий id, чтобы fetch/test/save работали.
    let current_id = RwSignal::new(id.clone());

    // Активная закладка: "settings" | "models".
    let (active_tab, set_active_tab) = signal("settings".to_string());

    // Однократная загрузка существующего подключения.
    if let Some(conn_id) = id.clone() {
        wasm_bindgen_futures::spawn_local(async move {
            match fetch_connection(&conn_id).await {
                Ok(connection) => {
                    // Считываем allowed_models ДО перемещения полей (метод берёт &self).
                    vm.allowed_models.set(connection.allowed_models_list());
                    vm.image_input_models
                        .set(connection.image_input_models_list());
                    vm.code.set(connection.base.code);
                    vm.description.set(connection.base.description);
                    vm.comment.set(connection.base.comment.unwrap_or_default());
                    vm.provider_type
                        .set(connection.provider_type.as_str().to_string());
                    vm.api_endpoint.set(connection.api_endpoint);
                    vm.api_key.set(connection.api_key);
                    vm.model_name.set(connection.model_name);
                    vm.temperature.set(connection.temperature.to_string());
                    vm.max_tokens.set(connection.max_tokens.to_string());
                    vm.context_window.set(connection.context_window.to_string());
                    // Незаданная ставка остаётся пустой строкой, а не «0»: пустое поле
                    // означает «стоимость не считаем», ноль — бесплатную модель.
                    let price_text =
                        |value: Option<f64>| value.map(|v| v.to_string()).unwrap_or_default();
                    vm.price_in_per_mtok
                        .set(price_text(connection.price_in_per_mtok));
                    vm.price_out_per_mtok
                        .set(price_text(connection.price_out_per_mtok));
                    vm.price_cached_per_mtok
                        .set(price_text(connection.price_cached_per_mtok));
                    vm.currency.set(connection.currency.unwrap_or_default());
                    vm.is_primary.set(connection.is_primary);

                    if let Some(models_json) = connection.available_models {
                        if let Ok(models) =
                            serde_json::from_str::<Vec<serde_json::Value>>(&models_json)
                        {
                            vm.set_available_models.set(models);
                        }
                    }
                }
                Err(e) => vm.set_error.set(Some(e)),
            }
        });
    }

    // Save handler — сначала проверяем выбор моделей, потом пишем.
    let handle_save = move |_| {
        if let Err(msg) = vm.validate_models() {
            vm.set_error.set(Some(msg));
            return;
        }
        let dto = vm.build_save_dto(current_id.get());

        wasm_bindgen_futures::spawn_local(async move {
            match save_connection(dto).await {
                Ok(_) => on_saved.run(()),
                Err(e) => vm.set_error.set(Some(e)),
            }
        });
    };

    // Test connection handler.
    let handle_test = move |_| {
        let id_value = match current_id.get() {
            Some(v) => v,
            None => {
                vm.set_test_result.set(Some((
                    false,
                    "Сохраните подключение перед тестированием".to_string(),
                )));
                return;
            }
        };

        vm.set_is_testing.set(true);
        vm.set_test_result.set(None);

        wasm_bindgen_futures::spawn_local(async move {
            match test_connection(&id_value).await {
                Ok(result) => {
                    vm.set_test_result
                        .set(Some((result.success, result.message)));
                    vm.set_is_testing.set(false);
                }
                Err(e) => {
                    vm.set_test_result
                        .set(Some((false, format!("Ошибка: {}", e))));
                    vm.set_is_testing.set(false);
                }
            }
        });
    };

    // Fetch models handler — для нового подключения сначала автосохраняем черновик.
    let handle_fetch_models = move |_| {
        vm.set_is_fetching_models.set(true);
        vm.set_fetch_models_result.set(None);

        wasm_bindgen_futures::spawn_local(async move {
            // 1. Гарантируем наличие id: сохраняем черновик, если элемент ещё не создан.
            let id_value = match current_id.get() {
                Some(v) => v,
                None => {
                    let dto = vm.build_save_dto(None);
                    match save_connection(dto).await {
                        Ok(Some(new_id)) => {
                            current_id.set(Some(new_id.clone()));
                            new_id
                        }
                        Ok(None) => {
                            vm.set_fetch_models_result.set(Some((
                                false,
                                "Не удалось сохранить черновик подключения.".to_string(),
                            )));
                            vm.set_is_fetching_models.set(false);
                            return;
                        }
                        Err(_) => {
                            vm.set_fetch_models_result.set(Some((
                                false,
                                "Заполните код, наименование и API-ключ перед загрузкой моделей."
                                    .to_string(),
                            )));
                            vm.set_is_fetching_models.set(false);
                            return;
                        }
                    }
                }
            };

            // 2. Загружаем каталог моделей.
            match fetch_models_from_api(&id_value).await {
                Ok(response) => {
                    if response.success {
                        vm.set_available_models.set(response.models);
                        vm.set_fetch_models_result.set(Some((
                            true,
                            format!("Загружено {} моделей", response.count),
                        )));
                    } else {
                        vm.set_fetch_models_result
                            .set(Some((false, response.message)));
                    }
                    vm.set_is_fetching_models.set(false);
                }
                Err(e) => {
                    vm.set_fetch_models_result
                        .set(Some((false, format!("Ошибка: {}", e))));
                    vm.set_is_fetching_models.set(false);
                }
            }
        });
    };

    let is_edit_mode = Signal::derive(move || current_id.get().is_some());

    view! {
        <PageFrame page_id="a038_llm_connection--details" category=PAGE_CAT_DETAIL>

            // ---- Header ----
            <div class="page__header">
                <div class="page__header-left">
                    <h1 class="page__title">
                        {move || if is_edit_mode.get() { "Редактирование подключения LLM" } else { "Новое подключение LLM" }}
                    </h1>
                </div>
                <div class="page__header-right">
                    <Button appearance=ButtonAppearance::Primary on_click=handle_save>
                        {icon("save")}
                        " Сохранить"
                    </Button>
                    <Show when=move || is_edit_mode.get()>
                        <Button
                            appearance=ButtonAppearance::Secondary
                            on_click=handle_test
                            disabled=vm.is_testing
                        >
                            {icon("link")}
                            {move || if vm.is_testing.get() { " Тестирование..." } else { " Тест подключения" }}
                        </Button>
                    </Show>
                    <Button appearance=ButtonAppearance::Secondary on_click=move |_| on_cancel.run(())>
                        {icon("close")}
                        " Отмена"
                    </Button>
                </div>
            </div>

            {move || {
                vm.error
                    .get()
                    .map(|e| {
                        view! {
                            <div style="padding: 12px; margin-bottom: 16px; background: var(--color-error-50); border: 1px solid var(--color-error-100); border-radius: 8px;">
                                <span style="color: var(--color-error);">{e}</span>
                            </div>
                        }
                    })
            }}

            {move || {
                vm.test_result
                    .get()
                    .map(|(success, message)| {
                        let bg = if success { "var(--color-success-50)" } else { "var(--color-error-50)" };
                        let border = if success { "var(--color-success-100)" } else { "var(--color-error-100)" };
                        let color = if success { "var(--color-success)" } else { "var(--color-error)" };
                        view! {
                            <div style=format!(
                                "padding: 12px; margin-bottom: 16px; background: {}; border: 1px solid {}; border-radius: 8px;",
                                bg, border,
                            )>
                                <span style=format!("color: {};", color)>{message}</span>
                            </div>
                        }
                    })
            }}

            // ---- Tab bar ----
            <div class="page__tabs">
                {["settings", "models"].into_iter().map(|tab| {
                    let tab_s = tab.to_string();
                    let label = match tab {
                        "settings" => "Настройки",
                        "models"   => "Модели",
                        _          => tab,
                    };
                    view! {
                        <button
                            class="page__tab"
                            class:page__tab--active=move || active_tab.get() == tab_s
                            on:click={
                                let tab_s2 = tab.to_string();
                                move |_| set_active_tab.set(tab_s2.clone())
                            }
                        >
                            {label}
                        </button>
                    }
                }).collect_view()}
            </div>

            // ---- Settings tab ----
            <div class="page__content" style=move || if active_tab.get() != "settings" { "display:none;" } else { "" }>
            <div style="display: grid; grid-template-columns: 500px 500px; gap: var(--spacing-md); max-width: 1050px; align-items: start; align-content: start;">
                <Card>
                    <div class="form__group">
                        <label class="form__label">"Код"</label>
                        <Input value=vm.code placeholder="OPENROUTER-MAIN" />
                    </div>

                    <div class="form__group">
                        <label class="form__label">
                            "Наименование"
                            <span style="color: red;">"*"</span>
                        </label>
                        <Input value=vm.description placeholder="OpenRouter основное подключение" />
                    </div>

                    <div class="form__group">
                        <label class="form__label">"Провайдер"</label>
                        <select
                            style="height: 32px; padding: 4px 8px; border: 1px solid var(--colorNeutralStroke2); border-radius: 6px; width: 100%; background: var(--color-surface); color: var(--color-text);"
                            prop:value=move || vm.provider_type.get()
                            on:change=move |ev| {
                                let provider = event_target_value(&ev);
                                let previous_endpoint = vm.api_endpoint.get();
                                let previous_model = vm.model_name.get();
                                vm.provider_type.set(provider.clone());

                                if provider == "OpenRouter" {
                                    vm.api_endpoint.set("https://openrouter.ai/api/v1".to_string());
                                    if previous_model.trim().is_empty() || previous_model == "gpt-4o" {
                                        vm.model_name.set("openai/gpt-4o".to_string());
                                    }
                                } else if provider == "OpenAI" {
                                    if previous_endpoint.trim().is_empty()
                                        || previous_endpoint == "https://openrouter.ai/api/v1"
                                    {
                                        vm.api_endpoint.set("https://api.openai.com/v1".to_string());
                                    }
                                    if previous_model.trim().is_empty() || previous_model == "openai/gpt-4o" {
                                        vm.model_name.set("gpt-4o".to_string());
                                    }
                                } else if provider == "DeepSeek" {
                                    vm.api_endpoint.set("https://api.deepseek.com".to_string());
                                    if previous_model.trim().is_empty()
                                        || previous_model == "gpt-4o"
                                        || previous_model == "openai/gpt-4o"
                                    {
                                        vm.model_name.set("deepseek-chat".to_string());
                                    }
                                } else if provider == "Kimi" {
                                    vm.api_endpoint.set("https://api.moonshot.ai/v1".to_string());
                                    if previous_model.trim().is_empty()
                                        || previous_model == "gpt-4o"
                                        || previous_model == "openai/gpt-4o"
                                        || previous_model == "deepseek-chat"
                                    {
                                        vm.model_name.set("kimi-k3".to_string());
                                    }
                                } else if provider == "Ollama" {
                                    vm.api_endpoint.set("http://localhost:11434/v1".to_string());
                                    if previous_model.trim().is_empty()
                                        || previous_model == "gpt-4o"
                                        || previous_model == "openai/gpt-4o"
                                        || previous_model == "deepseek-chat"
                                        || previous_model == "kimi-k3"
                                    {
                                        vm.model_name.set("gpt-oss:20b".to_string());
                                    }
                                    // Окно локальной модели на порядок меньше облачного:
                                    // дефолт 160000 здесь означал бы молчаливую обрезку промпта.
                                    if vm.context_window.get() == "160000" {
                                        vm.context_window.set("32768".to_string());
                                    }
                                }
                            }
                        >
                            <option value="OpenAI">"OpenAI"</option>
                            <option value="OpenRouter">"OpenRouter"</option>
                            <option value="DeepSeek">"DeepSeek"</option>
                            <option value="Kimi">"KIMI (Moonshot)"</option>
                            <option value="Ollama">"Ollama (локально)"</option>
                        </select>
                        <div style="font-size: 12px; color: var(--colorNeutralForeground3);">
                            "Реально поддерживаются OpenAI, OpenRouter (OpenAI-совместимый роутинг), DeepSeek, KIMI (Moonshot) и Ollama (локальная модель на этой машине)."
                        </div>
                    </div>

                    <div class="form__group">
                        <label class="form__label">"API Endpoint"</label>
                        <Input value=vm.api_endpoint placeholder="https://openrouter.ai/api/v1" />
                    </div>

                    <div class="form__group">
                        <label class="form__label">"Комментарий"</label>
                        <Textarea
                            value=vm.comment
                            placeholder="Используется для аналитики продаж"
                        />
                    </div>

                </Card>
                <Card>
                    <div class="form__group">
                        <label class="form__label">
                            "API Ключ"
                            <Show when=move || vm.provider_type.get() != "Ollama">
                                <span style="color: red;">"*"</span>
                            </Show>
                        </label>
                        <Input value=vm.api_key placeholder="sk-..." />
                        <Show when=move || vm.provider_type.get() == "Ollama">
                            <div style="font-size: 12px; color: var(--colorNeutralForeground3);">
                                "Локальной Ollama ключ не нужен — оставьте поле пустым."
                            </div>
                        </Show>
                    </div>

                    <div class="form__group">
                        <label class="form__label">"Temperature (0.0-2.0)"</label>
                        <Input value=vm.temperature placeholder="0.7" />
                    </div>

                    <div class="form__group">
                        <label class="form__label">"Max Tokens"</label>
                        <Input value=vm.max_tokens placeholder="4096" />
                    </div>

                    <div class="form__group">
                        <label class="form__label">"Размер контекста (токенов)"</label>
                        <Input value=vm.context_window placeholder="160000" />
                        <div style="font-size: 12px; color: var(--colorNeutralForeground3);">
                            "Окно контекста модели — из него считается бюджет истории чата. Для Ollama должно совпадать с OLLAMA_CONTEXT_LENGTH."
                        </div>
                    </div>

                    <div class="form__group">
                        <label class="form__label">"Прайс за 1M токенов"</label>
                        <div style="display: flex; gap: var(--spacing-sm); flex-wrap: wrap;">
                            <div style="flex: 1 1 120px;">
                                <Input value=vm.price_in_per_mtok placeholder="вход, 3.0" />
                            </div>
                            <div style="flex: 1 1 120px;">
                                <Input value=vm.price_out_per_mtok placeholder="выход, 15.0" />
                            </div>
                            <div style="flex: 1 1 120px;">
                                <Input value=vm.price_cached_per_mtok placeholder="кэш-вход, 0.3" />
                            </div>
                            <div style="flex: 0 1 90px;">
                                <Input value=vm.currency placeholder="RUB" />
                            </div>
                        </div>
                        <div style="font-size: 12px; color: var(--colorNeutralForeground3);">
                            "Пустые ставки — стоимость прогонов не считается (это не то же самое, что 0). Пустой кэш-вход означает, что скидки за кэш нет."
                        </div>
                    </div>

                    <div style="display: flex; align-items: center; gap: 8px;">
                        <input
                            type="checkbox"
                            prop:checked=move || vm.is_primary.get()
                            on:change=move |ev| {
                                let checked = event_target_checked(&ev);
                                vm.is_primary.set(checked);
                            }
                        />
                        <span>"Использовать как основное подключение"</span>
                    </div>

                </Card>

            </div>

            // ── Выбранные модели (только чтение; выбор — на закладке «Модели») ──────
            <div style="max-width: 1050px; margin-top: var(--spacing-md);">
                <Card>
                    {move || {
                        let allowed = vm.allowed_models.get();
                        let primary = vm.model_name.get();
                        view! {
                            <div style="display: flex; align-items: baseline; gap: 8px; margin-bottom: 8px;">
                                <label class="form__label" style="font-size: 16px; font-weight: 600;">"Выбранные модели"</label>
                                <span style="color: var(--colorNeutralForeground3);">{format!("({})", allowed.len())}</span>
                            </div>
                            {if allowed.is_empty() {
                                view! {
                                    <div style="color: var(--color-text-tertiary); font-size: 13px;">
                                        "Модели не выбраны — перейдите на закладку «Модели»."
                                    </div>
                                }.into_any()
                            } else {
                                view! {
                                    <div style="display: flex; flex-wrap: wrap; gap: 6px;">
                                        {allowed.into_iter().map(|mid| {
                                            let is_primary = mid == primary;
                                            let (bg, border) = if is_primary {
                                                ("var(--colorBrandBackground2)", "var(--colorBrandStroke1)")
                                            } else {
                                                ("var(--colorNeutralBackground3)", "var(--colorNeutralStroke2)")
                                            };
                                            view! {
                                                <span style=format!("display: inline-flex; align-items: center; gap: 4px; padding: 3px 10px; border-radius: 999px; font-size: 13px; background: {}; border: 1px solid {};", bg, border)>
                                                    {if is_primary { view! { <span style="color: var(--colorBrandForeground1);">"★"</span> }.into_any() } else { view! { <></> }.into_any() }}
                                                    {mid}
                                                </span>
                                            }
                                        }).collect_view()}
                                    </div>
                                }.into_any()
                            }}
                        }
                    }}
                </Card>
            </div>
            </div>

            // ---- Models tab ----
            <div class="page__content" style=move || if active_tab.get() != "models" { "display:none;" } else { "" }>
            <div style="display: flex; flex-direction: column; flex: 1; min-height: 0; width: 100%; max-width: 1400px; padding: var(--spacing-md);">
                <div style="display: flex; align-items: center; justify-content: space-between; gap: 12px; flex-wrap: wrap; margin-bottom: 8px;">
                    <label class="form__label" style="font-size: 16px; font-weight: 600;">"Модели"</label>
                    <div style="display: flex; align-items: center; gap: 8px;">
                        <input
                            type="text"
                            placeholder="Фильтр моделей..."
                            style="height: 32px; padding: 4px 8px; border: 1px solid var(--colorNeutralStroke2); border-radius: 6px; width: 220px; background: var(--color-surface); color: var(--color-text);"
                            prop:value=move || vm.model_filter.get()
                            on:input=move |ev| vm.model_filter.set(event_target_value(&ev))
                        />
                        <select
                            style="height: 32px; padding: 4px 8px; border: 1px solid var(--colorNeutralStroke2); border-radius: 6px; background: var(--color-surface); color: var(--color-text); max-width: 200px;"
                            prop:value=move || vm.provider_filter.get()
                            on:change=move |ev| vm.provider_filter.set(event_target_value(&ev))
                        >
                            <option value="">"Все провайдеры"</option>
                            {move || vm.available_providers().into_iter().map(|p| {
                                let label = p.clone();
                                view! { <option value=p>{label}</option> }
                            }).collect_view()}
                        </select>
                        <Button
                            appearance=ButtonAppearance::Primary
                            on_click=handle_fetch_models
                            disabled=vm.is_fetching_models
                        >
                            {icon("refresh")}
                            {move || if vm.is_fetching_models.get() { " Загрузка..." } else { " Получить список моделей" }}
                        </Button>
                    </div>
                </div>

                <div style="font-size: 12px; color: var(--colorNeutralForeground3); margin-bottom: 8px;">
                    "Отметьте чекбоксом 3-8 технически совместимых моделей — только они будут доступны в чате. Звёздочкой ★ выберите основную модель по умолчанию (она автоматически становится разрешённой)."
                </div>

                {move || {
                    vm.fetch_models_result
                        .get()
                        .map(|(success, message)| {
                            let color = if success { "var(--color-success)" } else { "var(--color-error)" };
                            view! {
                                <div style=format!("font-size: 13px; margin-bottom: 8px; color: {};", color)>
                                    {message}
                                </div>
                            }
                        })
                }}

                {move || {
                    let rows = vm.visible_model_rows();
                    if rows.is_empty() {
                        return view! {
                            <div style="color: var(--color-text-tertiary); padding: 12px 0;">
                                "Каталог моделей пуст — нажмите «Получить список моделей»."
                            </div>
                        }
                        .into_any();
                    }
                    view! {
                        <div style="flex: 1; min-height: 0; overflow-y: auto; border: 1px solid var(--colorNeutralStroke2); border-radius: 6px;">
                            <table style="width: 100%; border-collapse: collapse; font-size: 13px;">
                                <thead style="position: sticky; top: 0; background: var(--color-surface); z-index: 1;">
                                    <tr style="border-bottom: 1px solid var(--colorNeutralStroke2);">
                                        <th style="padding: 6px 8px; text-align: center; width: 70px;">"Разреш."</th>
                                        <th style="padding: 6px 8px; text-align: center; width: 70px;" title="Модель принимает изображения">"Изобр."</th>
                                        <th style="padding: 6px 8px; text-align: center; width: 60px;">"Осн."</th>
                                        <th
                                            style="padding: 6px 8px; text-align: left; cursor: pointer;"
                                            on:click=move |_| vm.toggle_sort(ModelSortCol::Id)
                                        >{move || sort_label(vm, ModelSortCol::Id, "Модель (id)")}</th>
                                        <th
                                            style="padding: 6px 8px; text-align: left; cursor: pointer; width: 130px;"
                                            on:click=move |_| vm.toggle_sort(ModelSortCol::Provider)
                                        >{move || sort_label(vm, ModelSortCol::Provider, "Провайдер")}</th>
                                        <th
                                            style="padding: 6px 8px; text-align: left; cursor: pointer;"
                                            on:click=move |_| vm.toggle_sort(ModelSortCol::Name)
                                        >{move || sort_label(vm, ModelSortCol::Name, "Название")}</th>
                                        <th
                                            style="padding: 6px 8px; text-align: right; cursor: pointer; width: 110px;"
                                            on:click=move |_| vm.toggle_sort(ModelSortCol::Context)
                                        >{move || sort_label(vm, ModelSortCol::Context, "Контекст")}</th>
                                        <th
                                            style="padding: 6px 8px; text-align: right; cursor: pointer; width: 120px;"
                                            title="Стоимость входящих токенов за 1 млн"
                                            on:click=move |_| vm.toggle_sort(ModelSortCol::PriceIn)
                                        >{move || sort_label(vm, ModelSortCol::PriceIn, "Вход $/1M")}</th>
                                        <th
                                            style="padding: 6px 8px; text-align: right; cursor: pointer; width: 120px;"
                                            title="Стоимость исходящих токенов за 1 млн"
                                            on:click=move |_| vm.toggle_sort(ModelSortCol::PriceOut)
                                        >{move || sort_label(vm, ModelSortCol::PriceOut, "Выход $/1M")}</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {rows.into_iter().map(|m| {
                                        let model_id = m.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                        let name = m.get("name").and_then(|v| v.as_str()).unwrap_or("—").to_string();
                                        let provider = {
                                            let p = model_provider(&m);
                                            if p.is_empty() { "—".to_string() } else { p }
                                        };
                                        let context = m.get("context_length")
                                            .and_then(|v| v.as_i64())
                                            .map(|n| n.to_string())
                                            .unwrap_or_else(|| "—".to_string());
                                        let price_in = price_per_million(&m, "prompt");
                                        let price_out = price_per_million(&m, "completion");
                                        // OpenRouter-style id (author/slug) → страница модели.
                                        let model_link = if model_id.contains('/') {
                                            Some(format!("https://openrouter.ai/{}", model_id))
                                        } else {
                                            None
                                        };
                                        let mid_check = model_id.clone();
                                        let mid_toggle = model_id.clone();
                                        let mid_image_check = model_id.clone();
                                        let mid_image_allowed = model_id.clone();
                                        let mid_image_toggle = model_id.clone();
                                        let mid_star = model_id.clone();
                                        let mid_star2 = model_id.clone();
                                        let mid_primary = model_id.clone();
                                        view! {
                                            <tr style="border-bottom: 1px solid var(--color-border-light);">
                                                <td style="padding: 6px 8px; text-align: center;">
                                                    <input
                                                        type="checkbox"
                                                        prop:checked=move || vm.allowed_models.get().contains(&mid_check)
                                                        on:change=move |_| vm.toggle_allowed(&mid_toggle)
                                                    />
                                                </td>
                                                <td style="padding: 6px 8px; text-align: center;">
                                                    <input
                                                        type="checkbox"
                                                        title="Поддерживает изображения"
                                                        disabled=move || !vm.allowed_models.get().contains(&mid_image_allowed)
                                                        prop:checked=move || vm.image_input_models.get().contains(&mid_image_check)
                                                        on:change=move |_| vm.toggle_image_input(&mid_image_toggle)
                                                    />
                                                </td>
                                                <td style="padding: 6px 8px; text-align: center;">
                                                    <span
                                                        style=move || {
                                                            let selected = vm.model_name.get() == mid_star2;
                                                            format!(
                                                                "cursor: pointer; font-size: 22px; line-height: 1; color: {};",
                                                                if selected { "#f5b301" } else { "var(--colorNeutralForeground3)" },
                                                            )
                                                        }
                                                        title="Сделать основной моделью"
                                                        on:click=move |_| vm.set_primary(&mid_primary)
                                                    >
                                                        {move || if vm.model_name.get() == mid_star { "★" } else { "☆" }}
                                                    </span>
                                                </td>
                                                <td style="padding: 6px 8px; font-family: var(--font-mono, monospace);">
                                                    {match model_link {
                                                        Some(url) => view! {
                                                            <a
                                                                href=url
                                                                target="_blank"
                                                                rel="noopener"
                                                                style="color: var(--colorBrandForeground1); text-decoration: none;"
                                                                title="Открыть описание и особенности модели на OpenRouter"
                                                            >
                                                                {model_id}
                                                            </a>
                                                        }.into_any(),
                                                        None => view! { {model_id} }.into_any(),
                                                    }}
                                                </td>
                                                <td style="padding: 6px 8px;">{provider}</td>
                                                <td style="padding: 6px 8px;">{name}</td>
                                                <td style="padding: 6px 8px; text-align: right;">{context}</td>
                                                <td style="padding: 6px 8px; text-align: right; font-variant-numeric: tabular-nums;">{price_in}</td>
                                                <td style="padding: 6px 8px; text-align: right; font-variant-numeric: tabular-nums;">{price_out}</td>
                                            </tr>
                                        }
                                    }).collect_view()}
                                </tbody>
                            </table>
                        </div>
                    }
                    .into_any()
                }}
            </div>
            </div>

        </PageFrame>
    }
}

/// Заголовок сортируемой колонки со стрелкой направления.
fn sort_label(vm: LlmConnectionDetailsVm, col: ModelSortCol, text: &str) -> String {
    let (active, asc) = vm.model_sort.get();
    if active == col {
        format!("{text} {}", if asc { "▲" } else { "▼" })
    } else {
        text.to_string()
    }
}

/// Стоимость токенов за 1 млн из `pricing.<key>` (провайдер отдаёт цену за 1 токен).
/// Для некорректных/отсутствующих данных возвращает пустую строку (ничего не показываем).
fn price_per_million(m: &serde_json::Value, key: &str) -> String {
    let raw = m.get("pricing").and_then(|p| p.get(key));
    let per_token = match raw {
        Some(serde_json::Value::String(s)) => s.trim().parse::<f64>().ok(),
        Some(serde_json::Value::Number(n)) => n.as_f64(),
        _ => None,
    };
    match per_token {
        Some(v) if v.is_finite() && v >= 0.0 => {
            let per_m = v * 1_000_000.0;
            if per_m >= 1.0 {
                format!("${:.2}", per_m)
            } else {
                // Дешёвые модели: больше знаков, чтобы не схлопнуть в $0.00.
                format!("${:.3}", per_m)
            }
        }
        _ => String::new(),
    }
}
