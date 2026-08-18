//! Projections Tab - Sales data (p904) projections

use super::super::api::TABLE_ID_PROJECTIONS;
use crate::shared::list_utils::{get_sort_class, get_sort_indicator};
use leptos::prelude::*;
use std::cmp::Ordering;

#[component]
pub fn ProjectionsTab(
    projections: Signal<Option<serde_json::Value>>,
    projections_loading: Signal<bool>,
    sort_column: Signal<Option<&'static str>>,
    set_sort_column: WriteSignal<Option<&'static str>>,
    sort_asc: Signal<bool>,
    set_sort_asc: WriteSignal<bool>,
) -> impl IntoView {
    view! {
        <div class="projections-info">
            {move || {
                if projections_loading.get() {
                    view! {
                        <div style="padding: var(--spacing-xl); text-align: center; color: var(--color-text-muted); font-size: var(--font-size-sm);">
                            "Загрузка проекций..."
                        </div>
                    }
                        .into_any()
                } else if let Some(proj_data) = projections.get() {
                    let mut p904_items = proj_data["p904_sales_data"]
                        .as_array()
                        .cloned()
                        .unwrap_or_default();
                    if let Some(col) = sort_column.get() {
                        let asc = sort_asc.get();
                        p904_items
                            .sort_by(|a, b| {
                                let cmp = match col {
                                    "article" => {
                                        let a_val = a["article"].as_str().unwrap_or("");
                                        let b_val = b["article"].as_str().unwrap_or("");
                                        a_val.cmp(b_val)
                                    }
                                    "date" => {
                                        let a_val = a["date"].as_str().unwrap_or("");
                                        let b_val = b["date"].as_str().unwrap_or("");
                                        a_val.cmp(b_val)
                                    }
                                    "price_list" => {
                                        let a_val = a["price_list"].as_f64().unwrap_or(0.0);
                                        let b_val = b["price_list"].as_f64().unwrap_or(0.0);
                                        a_val.partial_cmp(&b_val).unwrap_or(Ordering::Equal)
                                    }
                                    "price_return" => {
                                        let a_val = a["price_return"].as_f64().unwrap_or(0.0);
                                        let b_val = b["price_return"].as_f64().unwrap_or(0.0);
                                        a_val.partial_cmp(&b_val).unwrap_or(Ordering::Equal)
                                    }
                                    "customer_out" => {
                                        let a_val = a["customer_out"].as_f64().unwrap_or(0.0);
                                        let b_val = b["customer_out"].as_f64().unwrap_or(0.0);
                                        a_val.partial_cmp(&b_val).unwrap_or(Ordering::Equal)
                                    }
                                    "total" => {
                                        let a_val = a["total"].as_f64().unwrap_or(0.0);
                                        let b_val = b["total"].as_f64().unwrap_or(0.0);
                                        a_val.partial_cmp(&b_val).unwrap_or(Ordering::Equal)
                                    }
                                    _ => Ordering::Equal,
                                };
                                if asc { cmp } else { cmp.reverse() }
                            });
                    }
                    let handle_sort = move |column: &'static str| {
                        if sort_column.get() == Some(column) {
                            set_sort_asc.set(!sort_asc.get());
                        } else {
                            set_sort_column.set(Some(column));
                            set_sort_asc.set(true);
                        }
                    };
                                    view! {
                                        <div style="display: flex; flex-direction: column; gap: var(--spacing-lg);">
                            <div style="background: var(--color-bg-primary); padding: var(--spacing-lg); border-radius: var(--radius-md); box-shadow: var(--shadow-sm); border: 1px solid var(--color-border-lighter);">
                                <h3 style="margin: 0 0 var(--spacing-lg) 0; color: var(--color-text-primary); font-size: var(--font-size-base); font-weight: var(--font-weight-semibold); border-bottom: 2px solid var(--color-primary); padding-bottom: var(--spacing-md);">
                                    {format!("📈 Sales Data (p904) — {} записей", p904_items.len())}
                                </h3>
                                {if !p904_items.is_empty() {
                                    view! {
                                        <div class="table-container">
                                            <table class="table__data" id=TABLE_ID_PROJECTIONS>
                                                <thead>
                                                    <tr>
                                                        <th
                                                            class="resizable"
                                                            on:click=move |_| handle_sort("article")
                                                        >
                                                            <span class="table__sortable-header">
                                                                "Артикул"
                                                                <span class=move || {
                                                                    get_sort_class(
                                                                        sort_column.get().unwrap_or(""),
                                                                        "article",
                                                                    )
                                                                }>

                                                                    {move || {
                                                                        get_sort_indicator(
                                                                            sort_column.get().unwrap_or(""),
                                                                            "article",
                                                                            sort_asc.get(),
                                                                        )
                                                                    }}

                                                                </span>
                                                            </span>
                                                        </th>
                                                        <th class="resizable" on:click=move |_| handle_sort("date")>
                                                            <span class="table__sortable-header">
                                                                "Дата"
                                                                <span class=move || {
                                                                    get_sort_class(sort_column.get().unwrap_or(""), "date")
                                                                }>

                                                                    {move || {
                                                                        get_sort_indicator(
                                                                            sort_column.get().unwrap_or(""),
                                                                            "date",
                                                                            sort_asc.get(),
                                                                        )
                                                                    }}

                                                                </span>
                                                            </span>
                                                        </th>
                                                        <th
                                                            class="resizable text-right"
                                                            on:click=move |_| handle_sort("price_list")
                                                            title="price_list"
                                                        >
                                                            <span class="table__sortable-header">
                                                                "Цена прайс"
                                                                <span class=move || {
                                                                    get_sort_class(
                                                                        sort_column.get().unwrap_or(""),
                                                                        "price_list",
                                                                    )
                                                                }>

                                                                    {move || {
                                                                        get_sort_indicator(
                                                                            sort_column.get().unwrap_or(""),
                                                                            "price_list",
                                                                            sort_asc.get(),
                                                                        )
                                                                    }}

                                                                </span>
                                                            </span>
                                                        </th>
                                                        <th
                                                            class="resizable text-right"
                                                            on:click=move |_| handle_sort("price_return")
                                                            title="price_return"
                                                        >
                                                            <span class="table__sortable-header">
                                                                "Цена возврат"
                                                                <span class=move || {
                                                                    get_sort_class(
                                                                        sort_column.get().unwrap_or(""),
                                                                        "price_return",
                                                                    )
                                                                }>

                                                                    {move || {
                                                                        get_sort_indicator(
                                                                            sort_column.get().unwrap_or(""),
                                                                            "price_return",
                                                                            sort_asc.get(),
                                                                        )
                                                                    }}

                                                                </span>
                                                            </span>
                                                        </th>
                                                        <th
                                                            class="resizable text-right"
                                                            on:click=move |_| handle_sort("customer_out")
                                                            title="customer_out (отрицательное значение - возврат)"
                                                        >
                                                            <span class="table__sortable-header">
                                                                "К клиенту"
                                                                <span class=move || {
                                                                    get_sort_class(
                                                                        sort_column.get().unwrap_or(""),
                                                                        "customer_out",
                                                                    )
                                                                }>

                                                                    {move || {
                                                                        get_sort_indicator(
                                                                            sort_column.get().unwrap_or(""),
                                                                            "customer_out",
                                                                            sort_asc.get(),
                                                                        )
                                                                    }}

                                                                </span>
                                                            </span>
                                                        </th>
                                                        <th
                                                            class="resizable text-right"
                                                            on:click=move |_| handle_sort("total")
                                                            title="total"
                                                        >
                                                            <span class="table__sortable-header">
                                                                "Итого"
                                                                <span class=move || {
                                                                    get_sort_class(sort_column.get().unwrap_or(""), "total")
                                                                }>

                                                                    {move || {
                                                                        get_sort_indicator(
                                                                            sort_column.get().unwrap_or(""),
                                                                            "total",
                                                                            sort_asc.get(),
                                                                        )
                                                                    }}

                                                                </span>
                                                            </span>
                                                        </th>
                                                    </tr>
                                                </thead>
                                                <tbody>
                                                    {p904_items
                                                        .iter()
                                                        .map(|item| {
                                                            let article = item["article"].as_str().unwrap_or("—");
                                                            let date = item["date"].as_str().unwrap_or("—");
                                                            let date_formatted = if date.len() > 10 {
                                                                &date[..10]
                                                            } else {
                                                                date
                                                            };
                                                            let price_list = item["price_list"].as_f64().unwrap_or(0.0);
                                                            let price_return = item["price_return"]
                                                                .as_f64()
                                                                .unwrap_or(0.0);
                                                            let customer_out = item["customer_out"]
                                                                .as_f64()
                                                                .unwrap_or(0.0);
                                                            let total = item["total"].as_f64().unwrap_or(0.0);
                                                            view! {
                                                                <tr>
                                                                    <td style="font-family: monospace; font-size: var(--font-size-xs);">
                                                                        {article}
                                                                    </td>
                                                                    <td>{date_formatted}</td>
                                                                    <td class="text-right">{format!("{:.2}", price_list)}</td>
                                                                    <td class="text-right" style="color: #e65100;">
                                                                        {format!("{:.2}", price_return)}
                                                                    </td>
                                                                    <td
                                                                        class="text-right"
                                                                        style="color: #c62828; background: var(--color-error-50); font-weight: var(--font-weight-semibold);"
                                                                    >
                                                                        {format!("{:.2}", customer_out)}
                                                                    </td>
                                                                    <td class="text-right font-medium">
                                                                        {format!("{:.2}", total)}
                                                                    </td>
                                                                </tr>
                                                            }
                                                        })
                                                        .collect::<Vec<_>>()}

                                                </tbody>
                                            </table>
                                        </div>
                                    }
                                        .into_any()
                                } else {
                                    view! {
                                        <p style="text-align: center; padding: var(--spacing-lg); color: var(--color-text-muted); font-size: var(--font-size-sm);">
                                            "Нет записей. Документ должен иметь статус REFUNDED и быть проведён."
                                        </p>
                                    }
                                        .into_any()
                                }}

                            </div>
                        </div>
                    }
                        .into_any()
                } else {
                    view! {
                        <div style="padding: var(--spacing-xl); text-align: center; color: var(--color-text-muted); font-size: var(--font-size-sm);">
                            "Нет данных проекций"
                        </div>
                    }
                        .into_any()
                }
            }}

        </div>
    }
}
