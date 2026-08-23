use crate::shared::api_utils::api_base;
use crate::shared::date_utils::format_datetime;
use gloo_net::http::Request;
use leptos::logging::log;
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

// DTO структуры для детального представления
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OzonReturnsDetailDto {
    pub id: String,
    pub code: String,
    pub description: String,
    #[serde(rename = "connectionId")]
    pub connection_id: String,
    #[serde(rename = "organizationId")]
    pub organization_id: String,
    #[serde(rename = "marketplaceId")]
    pub marketplace_id: String,
    #[serde(rename = "returnId")]
    pub return_id: String,
    #[serde(rename = "returnDate")]
    pub return_date: String,
    #[serde(rename = "returnReasonName")]
    pub return_reason_name: String,
    #[serde(rename = "returnType")]
    pub return_type: String,
    #[serde(rename = "orderId")]
    pub order_id: String,
    #[serde(rename = "orderNumber")]
    pub order_number: String,
    pub sku: String,
    #[serde(rename = "productName")]
    pub product_name: String,
    pub price: f64,
    pub quantity: i32,
    #[serde(rename = "postingNumber")]
    pub posting_number: String,
    #[serde(rename = "clearingId")]
    pub clearing_id: Option<String>,
    #[serde(rename = "returnClearingId")]
    pub return_clearing_id: Option<String>,
    pub comment: Option<String>,
    pub metadata: MetadataDto,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataDto {
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
    #[serde(rename = "isDeleted")]
    pub is_deleted: bool,
    #[serde(rename = "isPosted")]
    pub is_posted: bool,
    pub version: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SalesRegisterDto {
    pub marketplace: String,
    pub document_no: String,
    pub line_id: String,
    pub scheme: Option<String>,
    pub document_type: String,
    pub sale_date: String,
    pub seller_sku: Option<String>,
    pub mp_item_id: String,
    pub title: Option<String>,
    pub qty: f64,
    pub price_effective: Option<f64>,
    pub amount_line: Option<f64>,
    pub currency_code: Option<String>,
    pub status_norm: String,
}

#[component]
pub fn OzonReturnsDetail(
    id: String,
    #[prop(into)] on_close: Callback<()>,
    #[prop(optional)] reload_trigger: Option<ReadSignal<u32>>,
) -> impl IntoView {
    let (return_data, set_return_data) = signal::<Option<OzonReturnsDetailDto>>(None);
    let (projections, set_projections) = signal::<Vec<SalesRegisterDto>>(Vec::new());
    let (projections_loading, set_projections_loading) = signal(false);
    let (loading, set_loading) = signal(true);
    let (error, set_error) = signal::<Option<String>>(None);
    let (active_tab, set_active_tab) = signal("general");
    let (posting_in_progress, set_posting_in_progress) = signal(false);

    // Клонируем id для использования в разных замыканиях
    let id_for_effect = id.clone();
    let id_for_view = id.clone();

    // Загрузить детальные данные
    Effect::new(move || {
        // Отслеживаем reload_trigger если передан
        if let Some(trigger) = reload_trigger {
            let _ = trigger.get();
        }

        let id = id_for_effect.clone();
        wasm_bindgen_futures::spawn_local(async move {
            set_loading.set(true);
            set_error.set(None);

            let url = format!("{}/api/ozon_returns/{}", api_base(), id);

            match Request::get(&url).send().await {
                Ok(response) => {
                    let status = response.status();
                    if status == 200 {
                        match response.text().await {
                            Ok(text) => {
                                match serde_json::from_str::<OzonReturnsDetailDto>(&text) {
                                    Ok(data) => {
                                        let return_id = data.id.clone();
                                        set_return_data.set(Some(data));
                                        set_loading.set(false);

                                        // Асинхронная загрузка проекций p900
                                        let set_projections = set_projections.clone();
                                        let set_projections_loading =
                                            set_projections_loading.clone();
                                        wasm_bindgen_futures::spawn_local(async move {
                                            set_projections_loading.set(true);
                                            let projections_url = format!(
                                                "{}/api/projections/p900/{}",
                                                api_base(),
                                                return_id
                                            );
                                            match Request::get(&projections_url).send().await {
                                                Ok(resp) => {
                                                    if resp.status() == 200 {
                                                        if let Ok(text) = resp.text().await {
                                                            if let Ok(items) = serde_json::from_str::<
                                                                Vec<SalesRegisterDto>,
                                                            >(
                                                                &text
                                                            ) {
                                                                set_projections.set(items);
                                                            }
                                                        }
                                                    }
                                                }
                                                Err(e) => {
                                                    log!("Failed to load projections: {:?}", e);
                                                }
                                            }
                                            set_projections_loading.set(false);
                                        });
                                    }
                                    Err(e) => {
                                        log!("Failed to parse return detail: {:?}", e);
                                        set_error.set(Some(format!("Ошибка парсинга: {}", e)));
                                        set_loading.set(false);
                                    }
                                }
                            }
                            Err(e) => {
                                log!("Failed to get text from response: {:?}", e);
                                set_error.set(Some(format!("Ошибка чтения ответа: {}", e)));
                                set_loading.set(false);
                            }
                        }
                    } else {
                        log!("Failed to load return detail, status: {}", status);
                        set_error.set(Some(format!("HTTP {}", status)));
                        set_loading.set(false);
                    }
                }
                Err(e) => {
                    log!("Failed to send request: {:?}", e);
                    set_error.set(Some(format!("Ошибка сети: {}", e)));
                    set_loading.set(false);
                }
            }
        });
    });

    view! {
        <div id="a009_ozon_returns--detail" data-page-category="legacy" class="ozon-returns-detail" style="padding: 20px; height: 100%; display: flex; flex-direction: column;">
            <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 20px; flex-shrink: 0;">
                <h2 style="margin: 0;">"Возврат OZON"</h2>
                <button
                    on:click=move |_| on_close.run(())
                    style="padding: 8px 16px; background: var(--color-error); color: #ffffff; border: none; border-radius: 4px; cursor: pointer;"
                >
                    "✕ Закрыть"
                </button>
            </div>

            <div style="flex: 1; overflow-y: auto; min-height: 0;">
                {move || {
                    if loading.get() {
                        view! {
                            <div style="text-align: center; padding: 40px;">
                                <p>"Загрузка..."</p>
                            </div>
                        }.into_any()
                    } else if let Some(err) = error.get() {
                        view! {
                            <div style="padding: 20px; background: var(--badge-error-bg); border: 1px solid var(--badge-error-border); border-radius: 4px; color: var(--badge-error-text);">
                                <strong>"Ошибка: "</strong>{err}
                            </div>
                        }.into_any()
                    } else if let Some(data) = return_data.get() {
                        view! {
                            <div style="height: 100%; display: flex; flex-direction: column;">
                                // Tabs
                                <div class="page__tabs" style="border-bottom: 2px solid var(--color-border); margin-bottom: 20px; flex-shrink: 0; position: sticky; top: 0; z-index: 10;">
                                    <button
                                        on:click=move |_| set_active_tab.set("general")
                                        style=move || format!(
                                            "padding: 10px 20px; border: none; border-radius: 4px 4px 0 0; cursor: pointer; margin-right: 5px; font-weight: 500; {}",
                                            if active_tab.get() == "general" {
                                                "background: var(--btn-primary-bg); color: var(--btn-primary-text); border-bottom: 2px solid var(--btn-primary-bg);"
                                            } else {
                                                "background: var(--color-bg-secondary); color: var(--color-text-secondary);"
                                            }
                                        )
                                    >
                                        "Основное"
                                    </button>
                                    <button
                                        on:click=move |_| set_active_tab.set("product")
                                        style=move || format!(
                                            "padding: 10px 20px; border: none; border-radius: 4px 4px 0 0; cursor: pointer; margin-right: 5px; font-weight: 500; {}",
                                            if active_tab.get() == "product" {
                                                "background: var(--btn-primary-bg); color: var(--btn-primary-text); border-bottom: 2px solid var(--btn-primary-bg);"
                                            } else {
                                                "background: var(--color-bg-secondary); color: var(--color-text-secondary);"
                                            }
                                        )
                                    >
                                        "Товар"
                                    </button>
                                    <button
                                        on:click=move |_| set_active_tab.set("metadata")
                                        style=move || format!(
                                            "padding: 10px 20px; border: none; border-radius: 4px 4px 0 0; cursor: pointer; margin-right: 5px; font-weight: 500; {}",
                                            if active_tab.get() == "metadata" {
                                                "background: var(--btn-primary-bg); color: var(--btn-primary-text); border-bottom: 2px solid var(--btn-primary-bg);"
                                            } else {
                                                "background: var(--color-bg-secondary); color: var(--color-text-secondary);"
                                            }
                                        )
                                    >
                                        "Метаданные"
                                    </button>
                                    <button
                                        on:click=move |_| set_active_tab.set("projections")
                                        style=move || format!(
                                            "padding: 10px 20px; border: none; border-radius: 4px 4px 0 0; cursor: pointer; margin-right: 5px; font-weight: 500; {}",
                                            if active_tab.get() == "projections" {
                                                "background: var(--btn-primary-bg); color: var(--btn-primary-text); border-bottom: 2px solid var(--btn-primary-bg);"
                                            } else {
                                                "background: var(--color-bg-secondary); color: var(--color-text-secondary);"
                                            }
                                        )
                                    >
                                        {move || format!("📊 Проекции ({})", projections.get().len())}
                                    </button>
                                </div>

                                // Tab content
                                <div style="flex: 1; overflow-y: auto; padding: 20px; background: var(--color-bg-secondary);">
                                    {
                                        let id_clone = id_for_view.clone();
                                        move || {
                                            let tab = active_tab.get();
                                            let data = data.clone();
                                            let current_id = id_clone.clone();
                                            match tab {
                                                "general" => render_general_tab(data).into_any(),
                                                "product" => render_product_tab(data).into_any(),
                                                "metadata" => render_metadata_tab(data).into_any(),
                                                "projections" => render_projections_tab(projections, projections_loading, data, posting_in_progress, set_posting_in_progress, current_id).into_any(),
                                                _ => view! { <div></div> }.into_any(),
                                            }
                                        }
                                    }
                                </div>
                            </div>
                        }.into_any()
                    } else {
                        view! { <div></div> }.into_any()
                    }
                }}
            </div>
        </div>
    }
}

// Вкладка "Основное"
fn render_general_tab(data: OzonReturnsDetailDto) -> impl IntoView {
    let total_amount = data.price * data.quantity as f64;
    let is_posted = data.metadata.is_posted;

    view! {
        <div style="display: flex; flex-direction: column; gap: 20px;">
            <div style="background: var(--card-bg); padding: 20px; border-radius: 8px; box-shadow: 0 1px 3px rgba(0,0,0,0.1);">
                <h3 style="margin: 0 0 15px 0; color: var(--color-text-primary); font-size: 16px; font-weight: 600; border-bottom: 2px solid var(--color-primary); padding-bottom: 8px;">"Информация о возврате"</h3>
                <div style="display: grid; grid-template-columns: 200px 1fr; gap: 15px 20px; align-items: center;">
                    <div style="font-weight: 600; color: var(--color-text-secondary);">"ID возврата:"</div>
                    <div style="font-family: 'Segoe UI', system-ui, sans-serif; font-size: 14px;">{data.return_id.clone()}</div>

                    <div style="font-weight: 600; color: var(--color-text-secondary);">"Дата возврата:"</div>
                    <div style="font-family: 'Segoe UI', system-ui, sans-serif; font-size: 14px;">{data.return_date.clone()}</div>

                    <div style="font-weight: 600; color: var(--color-text-secondary);">"Тип возврата:"</div>
                    <div style="font-family: 'Segoe UI', system-ui, sans-serif; font-size: 14px;">
                        <span style="padding: 2px 8px; background: var(--badge-primary-bg); color: var(--badge-primary-text); border-radius: 3px; font-weight: 500;">
                            {data.return_type.clone()}
                        </span>
                    </div>

                    <div style="font-weight: 600; color: var(--color-text-secondary);">"Причина возврата:"</div>
                    <div style="font-family: 'Segoe UI', system-ui, sans-serif; font-size: 14px;">{data.return_reason_name.clone()}</div>

                    <div style="font-weight: 600; color: var(--color-text-secondary);">"Проведен:"</div>
                    <div style="font-family: 'Segoe UI', system-ui, sans-serif; font-size: 14px;">
                        {if is_posted {
                            view! {
                                <span style="padding: 2px 8px; background: var(--badge-success-bg); color: var(--badge-success-text); border-radius: 3px; font-weight: 500;">
                                    "✓ Да"
                                </span>
                            }
                        } else {
                            view! {
                                <span style="padding: 2px 8px; background: var(--badge-neutral-bg); color: var(--badge-neutral-text); border-radius: 3px; font-weight: 500;">
                                    "○ Нет"
                                </span>
                            }
                        }}
                    </div>
                </div>
            </div>

            <div style="background: var(--card-bg); padding: 20px; border-radius: 8px; box-shadow: 0 1px 3px rgba(0,0,0,0.1);">
                <h3 style="margin: 0 0 15px 0; color: var(--color-text-primary); font-size: 16px; font-weight: 600; border-bottom: 2px solid var(--color-primary); padding-bottom: 8px;">"Информация о заказе"</h3>
                <div style="display: grid; grid-template-columns: 200px 1fr; gap: 15px 20px; align-items: center;">
                    <div style="font-weight: 600; color: var(--color-text-secondary);">"ID заказа:"</div>
                    <div style="font-family: 'Segoe UI', system-ui, sans-serif; font-size: 14px;">{data.order_id.clone()}</div>

                    <div style="font-weight: 600; color: var(--color-text-secondary);">"Номер заказа:"</div>
                    <div style="font-family: 'Segoe UI', system-ui, sans-serif; font-size: 14px;">{data.order_number.clone()}</div>

                    <div style="font-weight: 600; color: var(--color-text-secondary);">"Номер отправления:"</div>
                    <div style="font-family: 'Segoe UI', system-ui, sans-serif; font-size: 14px;">{data.posting_number.clone()}</div>
                </div>
            </div>

            <div style="background: var(--card-bg); padding: 20px; border-radius: 8px; box-shadow: 0 1px 3px rgba(0,0,0,0.1);">
                <h3 style="margin: 0 0 15px 0; color: var(--color-text-primary); font-size: 16px; font-weight: 600; border-bottom: 2px solid var(--color-success); padding-bottom: 8px;">"Финансовая информация"</h3>
                <div style="display: grid; grid-template-columns: 200px 1fr; gap: 15px 20px; align-items: center;">
                    <div style="font-weight: 600; color: var(--color-text-secondary);">"Сумма возврата:"</div>
                    <div style="font-family: 'Segoe UI', system-ui, sans-serif; font-size: 14px;">
                        <span style="color: var(--color-error); font-weight: 600; font-size: 18px;">{format!("−{:.2} ₽", total_amount)}</span>
                    </div>

                    <div style="font-weight: 600; color: var(--color-text-secondary);">"Clearing ID:"</div>
                    <div style="font-family: 'Segoe UI', system-ui, sans-serif; font-size: 14px;">{data.clearing_id.clone().unwrap_or_else(|| "—".to_string())}</div>

                    <div style="font-weight: 600; color: var(--color-text-secondary);">"Return Clearing ID:"</div>
                    <div style="font-family: 'Segoe UI', system-ui, sans-serif; font-size: 14px;">{data.return_clearing_id.clone().unwrap_or_else(|| "—".to_string())}</div>
                </div>
            </div>

            <div style="background: var(--card-bg); padding: 20px; border-radius: 8px; box-shadow: 0 1px 3px rgba(0,0,0,0.1);">
                <h3 style="margin: 0 0 15px 0; color: var(--color-text-primary); font-size: 16px; font-weight: 600; border-bottom: 2px solid var(--color-warning); padding-bottom: 8px;">"UUID связей"</h3>
                <div style="display: grid; grid-template-columns: 200px 1fr; gap: 15px 20px; align-items: center;">
                    <div style="font-weight: 600; color: var(--color-text-secondary);">"Connection ID:"</div>
                    <div style="display: flex; align-items: center; gap: 8px; font-family: monospace; font-size: 14px;">
                        <span style="color: var(--color-text-secondary);" title={data.connection_id.clone()}>{format!("{}...", data.connection_id.chars().take(8).collect::<String>())}</span>
                        <button
                            on:click={
                                let conn_id = data.connection_id.clone();
                                move |_| {
                                    let uuid_copy = conn_id.clone();
                                    wasm_bindgen_futures::spawn_local(async move {
                                        if let Some(window) = web_sys::window() {
                                            let nav = window.navigator().clipboard();
                                            let _ = nav.write_text(&uuid_copy);
                                        }
                                    });
                                }
                            }
                            style="padding: 2px 6px; font-size: 11px; border: 1px solid var(--color-border); background: var(--color-surface); color: var(--color-text-primary); border-radius: 3px; cursor: pointer;"
                            title="Copy to clipboard"
                        >
                            "📋"
                        </button>
                    </div>

                    <div style="font-weight: 600; color: var(--color-text-secondary);">"Organization ID:"</div>
                    <div style="display: flex; align-items: center; gap: 8px; font-family: monospace; font-size: 14px;">
                        <span style="color: var(--color-text-secondary);" title={data.organization_id.clone()}>{format!("{}...", data.organization_id.chars().take(8).collect::<String>())}</span>
                        <button
                            on:click={
                                let org_id = data.organization_id.clone();
                                move |_| {
                                    let uuid_copy = org_id.clone();
                                    wasm_bindgen_futures::spawn_local(async move {
                                        if let Some(window) = web_sys::window() {
                                            let nav = window.navigator().clipboard();
                                            let _ = nav.write_text(&uuid_copy);
                                        }
                                    });
                                }
                            }
                            style="padding: 2px 6px; font-size: 11px; border: 1px solid var(--color-border); background: var(--color-surface); color: var(--color-text-primary); border-radius: 3px; cursor: pointer;"
                            title="Copy to clipboard"
                        >
                            "📋"
                        </button>
                    </div>

                    <div style="font-weight: 600; color: var(--color-text-secondary);">"Marketplace ID:"</div>
                    <div style="display: flex; align-items: center; gap: 8px; font-family: monospace; font-size: 14px;">
                        <span style="color: var(--color-text-secondary);" title={data.marketplace_id.clone()}>{format!("{}...", data.marketplace_id.chars().take(8).collect::<String>())}</span>
                        <button
                            on:click={
                                let mp_id = data.marketplace_id.clone();
                                move |_| {
                                    let uuid_copy = mp_id.clone();
                                    wasm_bindgen_futures::spawn_local(async move {
                                        if let Some(window) = web_sys::window() {
                                            let nav = window.navigator().clipboard();
                                            let _ = nav.write_text(&uuid_copy);
                                        }
                                    });
                                }
                            }
                            style="padding: 2px 6px; font-size: 11px; border: 1px solid var(--color-border); background: var(--color-surface); color: var(--color-text-primary); border-radius: 3px; cursor: pointer;"
                            title="Copy to clipboard"
                        >
                            "📋"
                        </button>
                    </div>
                </div>
            </div>

            {data.comment.clone().map(|comment| {
                if !comment.is_empty() {
                    view! {
                        <div style="background: var(--card-bg); padding: 20px; border-radius: 8px; box-shadow: 0 1px 3px rgba(0,0,0,0.1);">
                            <h3 style="margin: 0 0 15px 0; color: var(--color-text-primary); font-size: 16px; font-weight: 600; border-bottom: 2px solid var(--color-accent); padding-bottom: 8px;">"Комментарий"</h3>
                            <p style="color: var(--color-text-secondary); line-height: 1.6; margin: 0;">{comment}</p>
                        </div>
                    }.into_any()
                } else {
                    view! { <div></div> }.into_any()
                }
            })}
        </div>
    }
}

// Вкладка "Товар"
fn render_product_tab(data: OzonReturnsDetailDto) -> impl IntoView {
    let total_amount = data.price * data.quantity as f64;

    view! {
        <div style="display: flex; flex-direction: column; gap: 20px;">
            <div style="background: var(--card-bg); padding: 20px; border-radius: 8px; box-shadow: 0 1px 3px rgba(0,0,0,0.1);">
                <h3 style="margin: 0 0 15px 0; color: var(--color-text-primary); font-size: 16px; font-weight: 600; border-bottom: 2px solid var(--color-primary); padding-bottom: 8px;">"Информация о товаре"</h3>
                <div style="display: grid; grid-template-columns: 200px 1fr; gap: 12px; align-items: center;">
                    <label style="font-weight: 500; color: var(--color-text-secondary);">"SKU:"</label>
                    <span style="color: var(--color-text-primary);">{data.sku.clone()}</span>
                </div>
                <div style="display: grid; grid-template-columns: 200px 1fr; gap: 12px; align-items: center;">
                    <label style="font-weight: 500; color: var(--color-text-secondary);">"Название:"</label>
                    <span style="color: var(--color-text-primary);">{data.product_name.clone()}</span>
                </div>
            </div>

            <div style="background: var(--card-bg); padding: 20px; border-radius: 8px; box-shadow: 0 1px 3px rgba(0,0,0,0.1);">
                <h3 style="margin: 0 0 15px 0; color: var(--color-text-primary); font-size: 16px; font-weight: 600; border-bottom: 2px solid var(--color-success); padding-bottom: 8px;">"Количество и цена"</h3>
                <div style="display: grid; grid-template-columns: 200px 1fr; gap: 12px; align-items: center;">
                    <label style="font-weight: 500; color: var(--color-text-secondary);">"Количество:"</label>
                    <span style="color: var(--color-text-primary);">{data.quantity}</span>
                </div>
                <div style="display: grid; grid-template-columns: 200px 1fr; gap: 12px; align-items: center;">
                    <label style="font-weight: 500; color: var(--color-text-secondary);">"Цена за единицу:"</label>
                    <span style="color: var(--color-text-primary);">{format!("{:.2} ₽", data.price)}</span>
                </div>
                <div style="display: grid; grid-template-columns: 200px 1fr; gap: 12px; align-items: center;">
                    <label style="font-weight: 500; color: var(--color-text-secondary);">"Общая сумма:"</label>
                    <span style="color: var(--color-success); font-weight: 600; font-size: 18px;">{format!("{:.2} ₽", total_amount)}</span>
                </div>
            </div>
        </div>
    }
}

// Вкладка "Метаданные"
fn render_metadata_tab(data: OzonReturnsDetailDto) -> impl IntoView {
    view! {
        <div style="display: flex; flex-direction: column; gap: 20px;">
            <div style="background: var(--card-bg); padding: 20px; border-radius: 8px; box-shadow: 0 1px 3px rgba(0,0,0,0.1);">
                <h3 style="margin: 0 0 15px 0; color: var(--color-text-primary); font-size: 16px; font-weight: 600; border-bottom: 2px solid var(--color-primary); padding-bottom: 8px;">"Системная информация"</h3>
                <div style="display: grid; grid-template-columns: 200px 1fr; gap: 12px; align-items: center;">
                    <label style="font-weight: 500; color: var(--color-text-secondary);">"ID записи:"</label>
                    <code style="font-size: 12px; background: var(--color-code-bg); color: var(--code-box-text); padding: 4px 8px; border-radius: 4px; font-family: monospace;">{data.id.clone()}</code>
                </div>
                <div style="display: grid; grid-template-columns: 200px 1fr; gap: 12px; align-items: center;">
                    <label style="font-weight: 500; color: var(--color-text-secondary);">"Код:"</label>
                    <span style="color: var(--color-text-primary);">{data.code.clone()}</span>
                </div>
                <div style="display: grid; grid-template-columns: 200px 1fr; gap: 12px; align-items: center;">
                    <label style="font-weight: 500; color: var(--color-text-secondary);">"Описание:"</label>
                    <span style="color: var(--color-text-primary);">{data.description.clone()}</span>
                </div>
            </div>

            <div style="background: var(--card-bg); padding: 20px; border-radius: 8px; box-shadow: 0 1px 3px rgba(0,0,0,0.1);">
                <h3 style="margin: 0 0 15px 0; color: var(--color-text-primary); font-size: 16px; font-weight: 600; border-bottom: 2px solid var(--color-warning); padding-bottom: 8px;">"Временные метки"</h3>
                <div style="display: grid; grid-template-columns: 200px 1fr; gap: 12px; align-items: center;">
                    <label style="font-weight: 500; color: var(--color-text-secondary);">"Создано:"</label>
                    <span style="color: var(--color-text-primary);">{format_datetime(&data.metadata.created_at)}</span>
                </div>
                <div style="display: grid; grid-template-columns: 200px 1fr; gap: 12px; align-items: center;">
                    <label style="font-weight: 500; color: var(--color-text-secondary);">"Обновлено:"</label>
                    <span style="color: var(--color-text-primary);">{format_datetime(&data.metadata.updated_at)}</span>
                </div>
            </div>

            <div style="background: var(--card-bg); padding: 20px; border-radius: 8px; box-shadow: 0 1px 3px rgba(0,0,0,0.1);">
                <h3 style="margin: 0 0 15px 0; color: var(--color-text-primary); font-size: 16px; font-weight: 600; border-bottom: 2px solid var(--color-accent); padding-bottom: 8px;">"Статусы"</h3>
                <div style="display: grid; grid-template-columns: 200px 1fr; gap: 12px; align-items: center;">
                    <label style="font-weight: 500; color: var(--color-text-secondary);">"Версия:"</label>
                    <span style="color: var(--color-text-primary);">{data.metadata.version}</span>
                </div>
                <div style="display: grid; grid-template-columns: 200px 1fr; gap: 12px; align-items: center;">
                    <label style="font-weight: 500; color: var(--color-text-secondary);">"Проведен:"</label>
                    <span style=move || {
                        if data.metadata.is_posted {
                            "display: inline-block; padding: 4px 12px; background: var(--badge-success-bg); color: var(--badge-success-text); border-radius: 12px; font-size: 13px;"
                        } else {
                            "display: inline-block; padding: 4px 12px; background: var(--badge-neutral-bg); color: var(--badge-neutral-text); border-radius: 12px; font-size: 13px;"
                        }
                    }>
                        {if data.metadata.is_posted { "Да" } else { "Нет" }}
                    </span>
                </div>
                <div style="display: grid; grid-template-columns: 200px 1fr; gap: 12px; align-items: center;">
                    <label style="font-weight: 500; color: var(--color-text-secondary);">"Удален:"</label>
                    <span style=move || {
                        if data.metadata.is_deleted {
                            "display: inline-block; padding: 4px 12px; background: var(--badge-error-bg); color: var(--badge-error-text); border-radius: 12px; font-size: 13px;"
                        } else {
                            "display: inline-block; padding: 4px 12px; background: var(--badge-success-bg); color: var(--badge-success-text); border-radius: 12px; font-size: 13px;"
                        }
                    }>
                        {if data.metadata.is_deleted { "Да" } else { "Нет" }}
                    </span>
                </div>
            </div>
        </div>
    }
}

// Вкладка "Проекции"
fn render_projections_tab(
    projections: ReadSignal<Vec<SalesRegisterDto>>,
    projections_loading: ReadSignal<bool>,
    data: OzonReturnsDetailDto,
    posting_in_progress: ReadSignal<bool>,
    set_posting_in_progress: WriteSignal<bool>,
    return_id: String,
) -> impl IntoView {
    let is_posted = data.metadata.is_posted;

    view! {
        <div class="projections-info" style="display: flex; flex-direction: column; gap: 20px;">
            // Управление проведением
            <div style="background: var(--card-bg); padding: 20px; border-radius: 8px; box-shadow: 0 1px 3px rgba(0,0,0,0.1);">
                <h3 style="margin: 0 0 15px 0; color: var(--color-text-primary); font-size: 16px; font-weight: 600; border-bottom: 2px solid var(--color-warning); padding-bottom: 8px;">"Управление проведением"</h3>
                <div style="display: flex; gap: 10px; align-items: center;">
                    {if is_posted {
                        view! {
                            <button
                                on:click=move |_| {
                                    let id = return_id.clone();
                                    set_posting_in_progress.set(true);
                                    wasm_bindgen_futures::spawn_local(async move {
                                        let url = format!("{}/api/a009/ozon-returns/{}/unpost", api_base(), id);
                                        match Request::post(&url).send().await {
                                            Ok(resp) if resp.status() == 200 => {
                                                log!("Document unposted successfully");
                                                if let Some(window) = web_sys::window() {
                                                    let _ = window.location().reload();
                                                }
                                            }
                                            Ok(resp) => {
                                                log!("Failed to unpost: HTTP {}", resp.status());
                                            }
                                            Err(e) => {
                                                log!("Error unposting document: {:?}", e);
                                            }
                                        }
                                        set_posting_in_progress.set(false);
                                    });
                                }
                                disabled=move || posting_in_progress.get()
                                style="padding: 10px 20px; background: var(--color-error); color: #ffffff; border: none; border-radius: 4px; cursor: pointer; font-weight: 500; font-size: 14px;"
                            >
                                {move || if posting_in_progress.get() { "⏳ Отмена..." } else { "✕ Отменить проведение" }}
                            </button>
                            <span style="padding: 4px 12px; background: var(--badge-success-bg); color: var(--badge-success-text); border-radius: 3px; font-weight: 500;">"✓ Проведен"</span>
                        }.into_any()
                    } else {
                        view! {
                            <button
                                on:click=move |_| {
                                    let id = return_id.clone();
                                    set_posting_in_progress.set(true);
                                    wasm_bindgen_futures::spawn_local(async move {
                                        let url = format!("{}/api/a009/ozon-returns/{}/post", api_base(), id);
                                        match Request::post(&url).send().await {
                                            Ok(resp) if resp.status() == 200 => {
                                                log!("Document posted successfully");
                                                if let Some(window) = web_sys::window() {
                                                    let _ = window.location().reload();
                                                }
                                            }
                                            Ok(resp) => {
                                                log!("Failed to post: HTTP {}", resp.status());
                                            }
                                            Err(e) => {
                                                log!("Error posting document: {:?}", e);
                                            }
                                        }
                                        set_posting_in_progress.set(false);
                                    });
                                }
                                disabled=move || posting_in_progress.get()
                                style="padding: 10px 20px; background: var(--color-success); color: #ffffff; border: none; border-radius: 4px; cursor: pointer; font-weight: 500; font-size: 14px;"
                            >
                                {move || if posting_in_progress.get() { "⏳ Проведение..." } else { "✓ Провести" }}
                            </button>
                            <span style="padding: 4px 12px; background: var(--badge-neutral-bg); color: var(--badge-neutral-text); border-radius: 3px; font-weight: 500;">"○ Не проведен"</span>
                        }.into_any()
                    }}
                </div>
            </div>

            // Список проекций
            {move || {
                if projections_loading.get() {
                    view! {
                        <div style="padding: 20px; text-align: center; color: var(--color-text-muted);">
                            "Загрузка проекций..."
                        </div>
                    }.into_any()
                } else {
                    let items = projections.get();
                    if items.is_empty() {
                        view! {
                            <div style="padding: 20px; text-align: center; color: var(--color-text-muted);">
                                "Нет записей в проекции p900"
                            </div>
                        }.into_any()
                    } else {
                        view! {
                            <div style="background: var(--card-bg); padding: 20px; border-radius: 8px; box-shadow: 0 1px 3px rgba(0,0,0,0.1);">
                                <div style="margin-bottom: 10px; padding: 10px; background: var(--info-box-bg); border: 1px solid var(--info-box-border); border-radius: 4px;">
                                    <strong>"Записи Sales Register (p900)"</strong>
                                    <span style="margin-left: 10px; color: var(--color-text-secondary);">{format!("Всего: {}", items.len())}</span>
                                </div>
                                <table style="width: 100%; border-collapse: collapse; font-size: 0.9em;">
                                    <thead>
                                        <tr style="background: var(--table-header-bg); color: var(--table-header-fg);">
                                            <th style="border: 1px solid var(--color-border); padding: 8px; text-align: left;">"#"</th>
                                            <th style="border: 1px solid var(--color-border); padding: 8px; text-align: left;">"Marketplace"</th>
                                            <th style="border: 1px solid var(--color-border); padding: 8px; text-align: left;">"Document №"</th>
                                            <th style="border: 1px solid var(--color-border); padding: 8px; text-align: left;">"SKU"</th>
                                            <th style="border: 1px solid var(--color-border); padding: 8px; text-align: left;">"Title"</th>
                                            <th style="border: 1px solid var(--color-border); padding: 8px; text-align: right;">"Qty"</th>
                                            <th style="border: 1px solid var(--color-border); padding: 8px; text-align: right;">"Amount"</th>
                                            <th style="border: 1px solid var(--color-border); padding: 8px; text-align: left;">"Sale Date"</th>
                                            <th style="border: 1px solid var(--color-border); padding: 8px; text-align: left;">"Status"</th>
                                        </tr>
                                    </thead>
                                    <tbody>
                                        {items.iter().enumerate().map(|(idx, item)| {
                                            // Отрицательные значения - подсветим красным
                                            let qty_style = if item.qty < 0.0 {
                                                "border: 1px solid var(--color-border); padding: 8px; text-align: right; color: var(--color-error); font-weight: bold;"
                                            } else {
                                                "border: 1px solid var(--color-border); padding: 8px; text-align: right;"
                                            };

                                            let amount_style = if item.amount_line.unwrap_or(0.0) < 0.0 {
                                                "border: 1px solid var(--color-border); padding: 8px; text-align: right; color: var(--color-error); font-weight: bold;"
                                            } else {
                                                "border: 1px solid var(--color-border); padding: 8px; text-align: right;"
                                            };

                                            view! {
                                                <tr>
                                                    <td style="border: 1px solid var(--color-border); padding: 8px;">{idx + 1}</td>
                                                    <td style="border: 1px solid var(--color-border); padding: 8px;">{item.marketplace.clone()}</td>
                                                    <td style="border: 1px solid var(--color-border); padding: 8px;"><code style="font-size: 0.85em;">{item.document_no.clone()}</code></td>
                                                    <td style="border: 1px solid var(--color-border); padding: 8px;"><code style="font-size: 0.85em;">{item.seller_sku.clone().unwrap_or("-".to_string())}</code></td>
                                                    <td style="border: 1px solid var(--color-border); padding: 8px;">{item.title.clone().unwrap_or("-".to_string())}</td>
                                                    <td style={qty_style}>{format!("{:.2}", item.qty)}</td>
                                                    <td style={amount_style}>
                                                        {item.amount_line.map(|a| format!("{:.2}", a)).unwrap_or("-".to_string())}
                                                        {item.currency_code.as_ref().map(|c| format!(" {}", c)).unwrap_or_default()}
                                                    </td>
                                                    <td style="border: 1px solid var(--color-border); padding: 8px;">{item.sale_date.clone()}</td>
                                                    <td style="border: 1px solid var(--color-border); padding: 8px;">
                                                        <span style="padding: 2px 8px; background: var(--badge-error-bg); color: var(--badge-error-text); border-radius: 3px; font-weight: 500;">
                                                            {item.status_norm.clone()}
                                                        </span>
                                                    </td>
                                                </tr>
                                            }
                                        }).collect_view()}
                                    </tbody>
                                </table>
                            </div>
                        }.into_any()
                    }
                }
            }}
        </div>
    }
}
