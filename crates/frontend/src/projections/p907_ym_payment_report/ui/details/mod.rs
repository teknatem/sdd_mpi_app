use crate::general_ledger::ui::{
    document_general_ledger_entries_nav_id, DocumentGeneralLedgerEntries,
};
use crate::layout::global_context::AppGlobalContext;
use crate::shared::icons::icon;
use crate::shared::json_viewer::widget::JsonViewer;
use crate::shared::list_utils::{get_sort_class, get_sort_indicator};
use crate::shared::page_frame::PageFrame;
use contracts::general_ledger::GeneralLedgerEntryDto;
use contracts::projections::p907_ym_payment_report::dto::{
    YmPaymentReportDetailResponse, YmPaymentReportDto,
};
use leptos::logging::log;
use leptos::prelude::*;
use leptos::task::spawn_local;
use std::collections::HashMap;
use thaw::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

#[derive(Debug, Clone)]
struct FieldRow {
    description: String,
    field_id: String,
    value: String,
}

/// Производные поля, добавленные системой (не приходят из YM API напрямую).
/// В UI выделяются серой рамкой слева, чтобы отличаться от оригинальных полей.
fn is_derived_field(field_id: &str) -> bool {
    matches!(
        field_id,
        "id" | "record_key"
            | "general_ledger_entries_count"
            | "marketplace_product_ref"
            | "marketplace_order_ref"
            | "nomenclature_ref"
    )
}

/// Тип (категория) реквизита. Производные поля помечаются как «Системный»,
/// реквизитам из YM API назначены смысловые категории.
fn field_type(field_id: &str) -> &'static str {
    if is_derived_field(field_id) {
        return "Системный";
    }
    match field_id {
        "connection_mp_ref" | "organization_ref" => "Метаданные",
        "business_id" | "partner_id" | "shop_name" | "inn" | "model" => "Бизнес",
        "transaction_id" | "transaction_date" | "transaction_type" | "transaction_source"
        | "transaction_sum" | "payment_status" => "Транзакция",
        "order_id"
        | "shop_order_id"
        | "order_creation_date"
        | "order_delivery_date"
        | "order_type" => "Заказ",
        "shop_sku" | "offer_or_service_name" | "count" => "Товар/услуга",
        "act_id" | "act_date" | "bank_order_id" | "bank_order_date" | "bank_sum" => "Банк/Акт",
        "claim_number" | "bonus_account_year_month" | "comments" => "Дополнительно",
        "loaded_at_utc" | "payload_version" => "Технические",
        _ => "Прочее",
    }
}

/// Реквизиты-ссылки (`*_ref`), для которых поддерживается резолв представления
/// через `/api/refs/resolve`. Имя реквизита совпадает с параметром `kind`.
fn is_resolvable_ref(field_id: &str) -> bool {
    matches!(
        field_id,
        "connection_mp_ref"
            | "organization_ref"
            | "nomenclature_ref"
            | "marketplace_product_ref"
            | "marketplace_order_ref"
    )
}

/// Извлекает из DTO пары (kind, uuid) для всех заполненных reference-полей.
fn ref_values(item: &YmPaymentReportDto) -> Vec<(&'static str, String)> {
    let mut out = Vec::new();
    let mut push = |kind: &'static str, value: &str| {
        let value = value.trim();
        if !value.is_empty() {
            out.push((kind, value.to_string()));
        }
    };
    push("connection_mp_ref", &item.connection_mp_ref);
    push("organization_ref", &item.organization_ref);
    if let Some(v) = item.marketplace_product_ref.as_deref() {
        push("marketplace_product_ref", v);
    }
    if let Some(v) = item.marketplace_order_ref.as_deref() {
        push("marketplace_order_ref", v);
    }
    if let Some(v) = item.nomenclature_ref.as_deref() {
        push("nomenclature_ref", v);
    }
    out
}

#[component]
pub fn YmPaymentReportDetail(
    /// Internal UUID of the record (used for API lookup and navigation).
    id: String,
    #[prop(into)] on_close: Callback<()>,
) -> impl IntoView {
    let tabs_store = use_context::<AppGlobalContext>().expect("AppGlobalContext not found");
    let (data, set_data) = signal::<Option<YmPaymentReportDto>>(None);
    let (general_ledger_entries, set_general_ledger_entries) =
        signal::<Vec<GeneralLedgerEntryDto>>(Vec::new());
    let (loading, set_loading) = signal(true);
    let (posting, set_posting) = signal(false);
    let (error, set_error) = signal(None::<String>);
    let (active_tab, set_active_tab) = signal("fields".to_string());
    let (sort_by, set_sort_by) = signal("description".to_string());
    let (sort_desc, set_sort_desc) = signal(false);

    // Представления reference-полей (kind → читаемое наименование), резолвятся лениво.
    let representations = RwSignal::new(HashMap::<String, String>::new());

    // p914 finance turnovers (lazy-loaded raw JSON)
    let (p914_json, set_p914_json) = signal::<Option<String>>(None);
    let (p914_loading, set_p914_loading) = signal(false);
    let (p914_error, set_p914_error) = signal(None::<String>);
    let (p914_fetched, set_p914_fetched) = signal(false);

    let id_clone = id.clone();
    Effect::new(move || {
        let id_val = id_clone.clone();
        spawn_local(async move {
            match fetch_detail(&id_val).await {
                Ok(response) => {
                    set_general_ledger_entries.set(response.general_ledger_entries);
                    set_data.set(Some(response.item));
                    set_loading.set(false);
                }
                Err(e) => {
                    log!("Failed to fetch YM payment report detail: {:?}", e);
                    set_error.set(Some(e));
                    set_loading.set(false);
                }
            }
        });
    });

    // Lazy-load p914 finance turnovers when the tab is first opened.
    let id_for_p914 = id.clone();
    Effect::new(move || {
        if active_tab.get() == "p914" && !p914_fetched.get_untracked() {
            set_p914_fetched.set(true);
            set_p914_loading.set(true);
            set_p914_error.set(None);
            let id_val = id_for_p914.clone();
            spawn_local(async move {
                match fetch_finance_turnovers_json(&id_val).await {
                    Ok(json) => {
                        set_p914_json.set(Some(json));
                        set_p914_loading.set(false);
                    }
                    Err(e) => {
                        log!("Failed to fetch p914 finance turnovers: {:?}", e);
                        set_p914_error.set(Some(e));
                        set_p914_loading.set(false);
                    }
                }
            });
        }
    });

    // Лениво резолвим представления reference-полей через /api/refs/resolve.
    Effect::new(move || {
        let Some(item) = data.get() else {
            return;
        };
        for (kind, id) in ref_values(&item) {
            if representations.with_untracked(|m| m.contains_key(kind)) {
                continue;
            }
            let kind_s = kind.to_string();
            let id_s = id.clone();
            spawn_local(async move {
                match fetch_representation(&kind_s, &id_s).await {
                    Ok(Some(rep)) => {
                        representations.update(|m| {
                            m.insert(kind_s, rep);
                        });
                    }
                    Ok(None) => {}
                    Err(e) => log!("Failed to resolve {} representation: {:?}", kind_s, e),
                }
            });
        }
    });

    let get_field_rows = move || -> Vec<FieldRow> {
        let Some(item) = data.get() else {
            return Vec::new();
        };

        let mut rows = vec![
            FieldRow {
                description: "UUID (внутренний ID)".to_string(),
                field_id: "id".to_string(),
                value: item.id.clone(),
            },
            FieldRow {
                description: "Ключ дедупликации".to_string(),
                field_id: "record_key".to_string(),
                value: item.record_key.clone(),
            },
            FieldRow {
                description: "ID транзакции (YM)".to_string(),
                field_id: "transaction_id".to_string(),
                value: item
                    .transaction_id
                    .clone()
                    .unwrap_or_else(|| "-".to_string()),
            },
            FieldRow {
                description: "Дата транзакции".to_string(),
                field_id: "transaction_date".to_string(),
                value: item
                    .transaction_date
                    .clone()
                    .unwrap_or_else(|| "-".to_string()),
            },
            FieldRow {
                description: "Тип транзакции".to_string(),
                field_id: "transaction_type".to_string(),
                value: item
                    .transaction_type
                    .clone()
                    .unwrap_or_else(|| "-".to_string()),
            },
            FieldRow {
                description: "Источник транзакции".to_string(),
                field_id: "transaction_source".to_string(),
                value: item
                    .transaction_source
                    .clone()
                    .unwrap_or_else(|| "-".to_string()),
            },
            FieldRow {
                description: "Сумма транзакции".to_string(),
                field_id: "transaction_sum".to_string(),
                value: item
                    .transaction_sum
                    .map(|v| format!("{:.2}", v))
                    .unwrap_or_else(|| "-".to_string()),
            },
            FieldRow {
                description: "Статус платежа".to_string(),
                field_id: "payment_status".to_string(),
                value: item
                    .payment_status
                    .clone()
                    .unwrap_or_else(|| "-".to_string()),
            },
            FieldRow {
                description: "ID заказа".to_string(),
                field_id: "order_id".to_string(),
                value: item
                    .order_id
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "-".to_string()),
            },
            FieldRow {
                description: "ID заказа магазина".to_string(),
                field_id: "shop_order_id".to_string(),
                value: item
                    .shop_order_id
                    .clone()
                    .unwrap_or_else(|| "-".to_string()),
            },
            FieldRow {
                description: "Дата создания заказа".to_string(),
                field_id: "order_creation_date".to_string(),
                value: item
                    .order_creation_date
                    .clone()
                    .unwrap_or_else(|| "-".to_string()),
            },
            FieldRow {
                description: "Дата доставки заказа".to_string(),
                field_id: "order_delivery_date".to_string(),
                value: item
                    .order_delivery_date
                    .clone()
                    .unwrap_or_else(|| "-".to_string()),
            },
            FieldRow {
                description: "Тип заказа".to_string(),
                field_id: "order_type".to_string(),
                value: item.order_type.clone().unwrap_or_else(|| "-".to_string()),
            },
            FieldRow {
                description: "SKU магазина".to_string(),
                field_id: "shop_sku".to_string(),
                value: item.shop_sku.clone().unwrap_or_else(|| "-".to_string()),
            },
            FieldRow {
                description: "Наименование товара / услуги".to_string(),
                field_id: "offer_or_service_name".to_string(),
                value: item
                    .offer_or_service_name
                    .clone()
                    .unwrap_or_else(|| "-".to_string()),
            },
            FieldRow {
                description: "Количество".to_string(),
                field_id: "count".to_string(),
                value: item
                    .count
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "-".to_string()),
            },
            FieldRow {
                description: "ID акта".to_string(),
                field_id: "act_id".to_string(),
                value: item
                    .act_id
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "-".to_string()),
            },
            FieldRow {
                description: "Дата акта".to_string(),
                field_id: "act_date".to_string(),
                value: item.act_date.clone().unwrap_or_else(|| "-".to_string()),
            },
            FieldRow {
                description: "ID банковского ордера".to_string(),
                field_id: "bank_order_id".to_string(),
                value: item
                    .bank_order_id
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "-".to_string()),
            },
            FieldRow {
                description: "Дата банковского ордера".to_string(),
                field_id: "bank_order_date".to_string(),
                value: item
                    .bank_order_date
                    .clone()
                    .unwrap_or_else(|| "-".to_string()),
            },
            FieldRow {
                description: "Сумма ПП (банковский перевод)".to_string(),
                field_id: "bank_sum".to_string(),
                value: item
                    .bank_sum
                    .map(|v| format!("{:.2}", v))
                    .unwrap_or_else(|| "-".to_string()),
            },
            FieldRow {
                description: "Номер претензии".to_string(),
                field_id: "claim_number".to_string(),
                value: item.claim_number.clone().unwrap_or_else(|| "-".to_string()),
            },
            FieldRow {
                description: "Год-месяц бонусного счёта".to_string(),
                field_id: "bonus_account_year_month".to_string(),
                value: item
                    .bonus_account_year_month
                    .clone()
                    .unwrap_or_else(|| "-".to_string()),
            },
            FieldRow {
                description: "Комментарий".to_string(),
                field_id: "comments".to_string(),
                value: item.comments.clone().unwrap_or_else(|| "-".to_string()),
            },
            FieldRow {
                description: "General ledger entries count".to_string(),
                field_id: "general_ledger_entries_count".to_string(),
                value: item.general_ledger_entries_count.to_string(),
            },
            FieldRow {
                description: "Товар маркетплейса (a007)".to_string(),
                field_id: "marketplace_product_ref".to_string(),
                value: item
                    .marketplace_product_ref
                    .clone()
                    .unwrap_or_else(|| "-".to_string()),
            },
            FieldRow {
                description: "Заказ (a013)".to_string(),
                field_id: "marketplace_order_ref".to_string(),
                value: item
                    .marketplace_order_ref
                    .clone()
                    .unwrap_or_else(|| "-".to_string()),
            },
            FieldRow {
                description: "Номенклатура (a004)".to_string(),
                field_id: "nomenclature_ref".to_string(),
                value: item
                    .nomenclature_ref
                    .clone()
                    .unwrap_or_else(|| "-".to_string()),
            },
            FieldRow {
                description: "ID бизнеса".to_string(),
                field_id: "business_id".to_string(),
                value: item
                    .business_id
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "-".to_string()),
            },
            FieldRow {
                description: "ID партнёра".to_string(),
                field_id: "partner_id".to_string(),
                value: item
                    .partner_id
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "-".to_string()),
            },
            FieldRow {
                description: "Название магазина".to_string(),
                field_id: "shop_name".to_string(),
                value: item.shop_name.clone().unwrap_or_else(|| "-".to_string()),
            },
            FieldRow {
                description: "ИНН".to_string(),
                field_id: "inn".to_string(),
                value: item.inn.clone().unwrap_or_else(|| "-".to_string()),
            },
            FieldRow {
                description: "Модель работы".to_string(),
                field_id: "model".to_string(),
                value: item.model.clone().unwrap_or_else(|| "-".to_string()),
            },
            FieldRow {
                description: "Подключение МП".to_string(),
                field_id: "connection_mp_ref".to_string(),
                value: item.connection_mp_ref.clone(),
            },
            FieldRow {
                description: "Организация".to_string(),
                field_id: "organization_ref".to_string(),
                value: item.organization_ref.clone(),
            },
            FieldRow {
                description: "Загружено (UTC)".to_string(),
                field_id: "loaded_at_utc".to_string(),
                value: item.loaded_at_utc.clone(),
            },
            FieldRow {
                description: "Версия payload".to_string(),
                field_id: "payload_version".to_string(),
                value: item.payload_version.to_string(),
            },
        ];

        let sort_field = sort_by.get();
        let is_desc = sort_desc.get();

        rows.sort_by(|a, b| {
            let cmp = match &*sort_field {
                "field_id" => a.field_id.cmp(&b.field_id),
                "field_type" => field_type(&a.field_id)
                    .cmp(field_type(&b.field_id))
                    .then_with(|| a.description.cmp(&b.description)),
                "value" => a.value.cmp(&b.value),
                _ => a.description.cmp(&b.description),
            }
            // Стабильный вторичный ключ: одинаковые значения упорядочиваем по id.
            .then_with(|| a.field_id.cmp(&b.field_id));
            if is_desc {
                cmp.reverse()
            } else {
                cmp
            }
        });

        rows
    };

    let handle_column_sort = move |column: &'static str| {
        if sort_by.get() == column {
            set_sort_desc.set(!sort_desc.get());
        } else {
            set_sort_by.set(column.to_string());
            set_sort_desc.set(false);
        }
    };

    let handle_post = {
        let id = id.clone();
        move |_| {
            if posting.get_untracked() {
                return;
            }
            set_posting.set(true);
            set_error.set(None);
            let id = id.clone();
            spawn_local(async move {
                match post_detail(&id).await {
                    Ok(response) => {
                        set_general_ledger_entries.set(response.general_ledger_entries);
                        set_data.set(Some(response.item));
                        set_active_tab.set("general_ledger".to_string());
                    }
                    Err(e) => {
                        log!("Failed to rebuild YM payment report GL: {:?}", e);
                        set_error.set(Some(e));
                    }
                }
                set_posting.set(false);
            });
        }
    };

    let export_to_excel = move || {
        let field_rows = get_field_rows();
        if field_rows.is_empty() {
            return;
        }

        let mut csv = String::from("\u{FEFF}");
        csv.push_str("Описание;Тип;Идентификатор;Значение\n");
        for row in field_rows {
            csv.push_str(&format!(
                "\"{}\";\"{}\";\"{}\";\"{}\"\n",
                row.description.replace('\"', "\"\""),
                field_type(&row.field_id).replace('\"', "\"\""),
                row.field_id.replace('\"', "\"\""),
                row.value.replace('\"', "\"\"")
            ));
        }

        use js_sys::Array;
        use wasm_bindgen::JsValue;

        let array = Array::new();
        array.push(&JsValue::from_str(&csv));
        let blob_props = web_sys::BlobPropertyBag::new();
        blob_props.set_type("text/csv;charset=utf-8;");
        if let Ok(blob) = web_sys::Blob::new_with_str_sequence_and_options(&array, &blob_props) {
            if let Ok(url) = web_sys::Url::create_object_url_with_blob(&blob) {
                if let Some(window) = web_sys::window() {
                    if let Some(document) = window.document() {
                        if let Ok(a) = document.create_element("a") {
                            let a: web_sys::HtmlAnchorElement = a.unchecked_into();
                            a.set_href(&url);
                            a.set_download(&format!(
                                "ym_payment_report_detail_{}.csv",
                                chrono::Utc::now().format("%Y%m%d_%H%M%S")
                            ));
                            let _ = a.click();
                            let _ = web_sys::Url::revoke_object_url(&url);
                        }
                    }
                }
            }
        }
    };

    view! {
        <PageFrame page_id="p907_ym_payment_report--detail" category="detail" class="p907-detail">
            <div class="modal-header">
                <h3 class="modal-title">"YM строка отчета"</h3>
                <div class="modal-header-actions">
                    <Button
                        appearance=ButtonAppearance::Primary
                        size=ButtonSize::Small
                        on_click=handle_post
                        disabled=Signal::derive(move || loading.get() || data.get().is_none() || posting.get())
                    >
                        {icon("refresh")}
                        {move || if posting.get() { " Проведение..." } else { " Провести GL" }}
                    </Button>
                    <Button
                        appearance=ButtonAppearance::Secondary
                        size=ButtonSize::Small
                        on_click=move |_| export_to_excel()
                        disabled=Signal::derive(move || loading.get() || data.get().is_none())
                    >
                        {icon("download")}
                        " Excel (csv)"
                    </Button>
                    <Button
                        appearance=ButtonAppearance::Secondary
                        size=ButtonSize::Small
                        on_click=move |_| on_close.run(())
                    >
                        {icon("x")}
                        " Закрыть"
                    </Button>
                </div>
            </div>

            <div class="page__content">
                <div class="detail-tabs">
                    <button
                        class=move || if active_tab.get() == "fields" {
                            "detail-tabs__item detail-tabs__item--active"
                        } else {
                            "detail-tabs__item"
                        }
                        on:click=move |_| set_active_tab.set("fields".to_string())
                    >
                        "Поля"
                    </button>
                    <button
                        class=move || if active_tab.get() == "general_ledger" {
                            "detail-tabs__item detail-tabs__item--active"
                        } else {
                            "detail-tabs__item"
                        }
                        on:click=move |_| set_active_tab.set("general_ledger".to_string())
                    >
                        "General Ledger"
                    </button>
                    <button
                        class=move || if active_tab.get() == "p914" {
                            "detail-tabs__item detail-tabs__item--active"
                        } else {
                            "detail-tabs__item"
                        }
                        on:click=move |_| set_active_tab.set("p914".to_string())
                    >
                        "p914 (fina) JSON"
                    </button>
                </div>

                {move || {
                    if loading.get() {
                        view! { <p class="text-muted">"Загрузка..."</p> }.into_any()
                    } else if let Some(err) = error.get() {
                        view! {
                            <div class="warning-box warning-box--error">
                                <span class="warning-box__icon">"⚠"</span>
                                <span class="warning-box__text">{err}</span>
                            </div>
                        }.into_any()
                    } else if active_tab.get() == "general_ledger" {
                        let entries = Signal::derive(move || general_ledger_entries.get());
                        view! {
                            <DocumentGeneralLedgerEntries
                                entries=entries
                                loading=Signal::derive(|| false)
                                error=Signal::derive(|| None::<String>)
                                nav_id=document_general_ledger_entries_nav_id("p907_ym_payment_report")
                                title="Журнал операций"
                                empty_message="Нет связанных записей general ledger. Проведите документ для формирования проводок."
                            />
                        }.into_any()
                    } else if active_tab.get() == "p914" {
                        if p914_loading.get() {
                            view! { <p class="text-muted">"Загрузка оборотов p914..."</p> }.into_any()
                        } else if let Some(err) = p914_error.get() {
                            view! {
                                <div class="warning-box warning-box--error">
                                    <span class="warning-box__icon">"⚠"</span>
                                    <span class="warning-box__text">{err}</span>
                                </div>
                            }.into_any()
                        } else {
                            let json_content = p914_json.get().unwrap_or_else(|| "[]".to_string());
                            view! {
                                <JsonViewer
                                    json_content=json_content
                                    title="p914_mp_finance_turnovers (слой fina)".to_string()
                                />
                            }.into_any()
                        }
                    } else if data.get().is_some() {
                        view! {
                            <div class="table-wrapper">
                                <Table attr:style="width: 100%;">
                                    <TableHeader>
                                        <TableRow>
                                            <TableHeaderCell resizable=true min_width=300.0>
                                                <span
                                                    class="p907-sort-header"
                                                    on:click=move |_| handle_column_sort("description")
                                                >
                                                    "Описание"
                                                    <span class={move || get_sort_class("description", &sort_by.get())}>
                                                        {move || get_sort_indicator("description", &sort_by.get(), !sort_desc.get())}
                                                    </span>
                                                </span>
                                            </TableHeaderCell>
                                            <TableHeaderCell resizable=true min_width=140.0>
                                                <span
                                                    class="p907-sort-header"
                                                    on:click=move |_| handle_column_sort("field_type")
                                                >
                                                    "Тип"
                                                    <span class={move || get_sort_class("field_type", &sort_by.get())}>
                                                        {move || get_sort_indicator("field_type", &sort_by.get(), !sort_desc.get())}
                                                    </span>
                                                </span>
                                            </TableHeaderCell>
                                            <TableHeaderCell resizable=true min_width=200.0>
                                                <span
                                                    class="p907-sort-header"
                                                    on:click=move |_| handle_column_sort("field_id")
                                                >
                                                    "Идентификатор"
                                                    <span class={move || get_sort_class("field_id", &sort_by.get())}>
                                                        {move || get_sort_indicator("field_id", &sort_by.get(), !sort_desc.get())}
                                                    </span>
                                                </span>
                                            </TableHeaderCell>
                                            <TableHeaderCell resizable=true min_width=200.0>
                                                <span
                                                    class="p907-sort-header"
                                                    on:click=move |_| handle_column_sort("value")
                                                >
                                                    "Значение"
                                                    <span class={move || get_sort_class("value", &sort_by.get())}>
                                                        {move || get_sort_indicator("value", &sort_by.get(), !sort_desc.get())}
                                                    </span>
                                                </span>
                                            </TableHeaderCell>
                                        </TableRow>
                                    </TableHeader>
                                    <TableBody>
                                        {move || {
                                            let reps = representations.get();
                                            get_field_rows()
                                            .into_iter()
                                            .map(move |row| {
                                                // Производные поля отмечаются серой полосой слева
                                                // (та же механика, что в spec-list).
                                                let row_style = if is_derived_field(&row.field_id) {
                                                    "--spec-cat:var(--color-border);"
                                                } else {
                                                    ""
                                                };
                                                let value = row.value.clone();
                                                let has_value = value != "-" && !value.is_empty();
                                                let type_label = field_type(&row.field_id);
                                                // Представление reference-поля (вторая строка под UUID).
                                                let representation = if has_value && is_resolvable_ref(&row.field_id) {
                                                    reps.get(&row.field_id).cloned()
                                                } else {
                                                    None
                                                };
                                                let link_target = match row.field_id.as_str() {
                                                    "marketplace_product_ref" if has_value => Some((
                                                        format!("a007_marketplace_product_details_{}", value),
                                                        format!("Товар {}", &value[..value.len().min(8)]),
                                                    )),
                                                    "marketplace_order_ref" if has_value => Some((
                                                        format!("a013_ym_order_details_{}", value),
                                                        format!("Заказ {}", &value[..value.len().min(8)]),
                                                    )),
                                                    _ => None,
                                                };
                                                let tabs_store = tabs_store.clone();
                                                view! {
                                                    <TableRow class="spec-list__row" attr:style=row_style>
                                                        <TableCell>
                                                            <TableCellLayout>
                                                                <span class="spec-list__name">{row.description}</span>
                                                            </TableCellLayout>
                                                        </TableCell>
                                                        <TableCell>
                                                            <TableCellLayout>
                                                                <span class="spec-list__name">{type_label}</span>
                                                            </TableCellLayout>
                                                        </TableCell>
                                                        <TableCell>
                                                            <TableCellLayout>
                                                                <span class="spec-list__ident">{row.field_id}</span>
                                                            </TableCellLayout>
                                                        </TableCell>
                                                        <TableCell>
                                                            <TableCellLayout>
                                                                <div class="p907-ref-value">
                                                                    {if let Some((tab_key, tab_label)) = link_target {
                                                                        let tabs_store = tabs_store.clone();
                                                                        view! {
                                                                            <a href="#" class="table__link" on:click=move |e| {
                                                                                e.prevent_default();
                                                                                tabs_store.open_tab(&tab_key, &tab_label);
                                                                            }>{value}</a>
                                                                        }.into_any()
                                                                    } else {
                                                                        view! {
                                                                            <span class="spec-list__name">{value}</span>
                                                                        }.into_any()
                                                                    }}
                                                                    {representation.map(|rep| view! {
                                                                        <div class="p907-ref-repr">{rep}</div>
                                                                    })}
                                                                </div>
                                                            </TableCellLayout>
                                                        </TableCell>
                                                    </TableRow>
                                                }.into_view()
                                            })
                                            .collect_view()
                                        }}
                                    </TableBody>
                                </Table>
                            </div>
                        }.into_any()
                    } else {
                        view! { <p class="text-muted">"Нет данных"</p> }.into_any()
                    }
                }}
            </div>
        </PageFrame>
    }
}

/// Резолвит человекочитаемое представление reference-поля через
/// `/api/refs/resolve?kind=<field_id>&id=<uuid>`.
async fn fetch_representation(kind: &str, id: &str) -> Result<Option<String>, String> {
    let window = web_sys::window().ok_or("No window object")?;
    // kind — фиксированный ASCII-идентификатор, id — UUID. URL-safe.
    let url = format!("/api/refs/resolve?kind={}&id={}", kind, id);

    let resp_value = JsFuture::from(window.fetch_with_str(&url))
        .await
        .map_err(|e| format!("Fetch failed: {:?}", e))?;

    let resp: web_sys::Response = resp_value
        .dyn_into()
        .map_err(|_| "Failed to cast to Response")?;

    if !resp.ok() {
        return Err(format!("HTTP error: {}", resp.status()));
    }

    let json = JsFuture::from(resp.json().map_err(|_| "Failed to get JSON")?)
        .await
        .map_err(|e| format!("Failed to parse JSON: {:?}", e))?;

    let parsed: ResolveRefResponse = serde_wasm_bindgen::from_value(json)
        .map_err(|e| format!("Failed to deserialize: {:?}", e))?;
    Ok(parsed.representation)
}

#[derive(serde::Deserialize)]
struct ResolveRefResponse {
    representation: Option<String>,
}

async fn fetch_detail(id: &str) -> Result<YmPaymentReportDetailResponse, String> {
    let window = web_sys::window().ok_or("No window object")?;
    // UUID contains only hex digits and hyphens — URL-safe, no encoding needed.
    let url = format!("/api/p907/payment-report/{}", id);

    let resp_value = JsFuture::from(window.fetch_with_str(&url))
        .await
        .map_err(|e| format!("Fetch failed: {:?}", e))?;

    let resp: web_sys::Response = resp_value
        .dyn_into()
        .map_err(|_| "Failed to cast to Response")?;

    if !resp.ok() {
        return Err(format!("HTTP error: {}", resp.status()));
    }

    let json = JsFuture::from(resp.json().map_err(|_| "Failed to get JSON")?)
        .await
        .map_err(|e| format!("Failed to parse JSON: {:?}", e))?;

    serde_wasm_bindgen::from_value(json).map_err(|e| format!("Failed to deserialize: {:?}", e))
}

/// Загружает строки проекции p914, относящиеся к данной записи p907, и
/// возвращает сырой JSON-текст ответа (массив `MpFinanceTurnoverDto`).
async fn fetch_finance_turnovers_json(id: &str) -> Result<String, String> {
    let window = web_sys::window().ok_or("No window object")?;
    let url = format!("/api/p907/payment-report/{}/finance-turnovers", id);

    let resp_value = JsFuture::from(window.fetch_with_str(&url))
        .await
        .map_err(|e| format!("Fetch failed: {:?}", e))?;

    let resp: web_sys::Response = resp_value
        .dyn_into()
        .map_err(|_| "Failed to cast to Response")?;

    if !resp.ok() {
        return Err(format!("HTTP error: {}", resp.status()));
    }

    let text = JsFuture::from(resp.text().map_err(|_| "Failed to get text")?)
        .await
        .map_err(|e| format!("Failed to read body: {:?}", e))?;

    text.as_string()
        .ok_or_else(|| "Response body is not a string".to_string())
}

async fn post_detail(id: &str) -> Result<YmPaymentReportDetailResponse, String> {
    use web_sys::{Request, RequestInit, RequestMode};

    let window = web_sys::window().ok_or("No window object")?;
    let url = format!("/api/p907/payment-report/{}/post", id);

    let opts = RequestInit::new();
    opts.set_method("POST");
    opts.set_mode(RequestMode::Cors);

    let request = Request::new_with_str_and_init(&url, &opts)
        .map_err(|e| format!("Request init failed: {:?}", e))?;

    let resp_value = JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("Fetch failed: {:?}", e))?;

    let resp: web_sys::Response = resp_value
        .dyn_into()
        .map_err(|_| "Failed to cast to Response")?;

    if !resp.ok() {
        return Err(format!("HTTP error: {}", resp.status()));
    }

    let json = JsFuture::from(resp.json().map_err(|_| "Failed to get JSON")?)
        .await
        .map_err(|e| format!("Failed to parse JSON: {:?}", e))?;

    serde_wasm_bindgen::from_value(json).map_err(|e| format!("Failed to deserialize: {:?}", e))
}
