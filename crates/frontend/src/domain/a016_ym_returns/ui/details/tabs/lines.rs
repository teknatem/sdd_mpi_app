//! Lines Tab - Return line items with sortable table

use super::super::api::{LineDto, TABLE_ID_LINES};
use crate::shared::list_utils::{get_sort_class, get_sort_indicator};
use leptos::prelude::*;
use std::cmp::Ordering;

#[component]
pub fn LinesTab(
    lines: Vec<LineDto>,
    sort_column: Signal<Option<&'static str>>,
    set_sort_column: WriteSignal<Option<&'static str>>,
    sort_asc: Signal<bool>,
    set_sort_asc: WriteSignal<bool>,
) -> impl IntoView {
    // Clone and sort lines
    let mut sorted_lines = lines.clone();
    if let Some(col) = sort_column.get() {
        let asc = sort_asc.get();
        sorted_lines.sort_by(|a, b| {
            let cmp = match col {
                "shop_sku" => a.shop_sku.cmp(&b.shop_sku),
                "name" => a.name.cmp(&b.name),
                "count" => a.count.cmp(&b.count),
                "price" => {
                    let a_price = a.price.unwrap_or(0.0);
                    let b_price = b.price.unwrap_or(0.0);
                    a_price.partial_cmp(&b_price).unwrap_or(Ordering::Equal)
                }
                _ => Ordering::Equal,
            };
            if asc {
                cmp
            } else {
                cmp.reverse()
            }
        });
    }

    let total_items: i32 = sorted_lines.iter().map(|l| l.count).sum();
    let total_amount: f64 = sorted_lines
        .iter()
        .filter_map(|l| l.price.map(|p| p * l.count as f64))
        .sum();

    // Sort handler
    let handle_sort = move |column: &'static str| {
        if sort_column.get() == Some(column) {
            set_sort_asc.set(!sort_asc.get());
        } else {
            set_sort_column.set(Some(column));
            set_sort_asc.set(true);
        }
    };

    view! {
        <div class="lines-info">
            <div style="margin-bottom: var(--spacing-lg); padding: var(--spacing-lg); background: var(--color-error-50); border-radius: var(--radius-sm); font-size: var(--font-size-sm);">
                <strong>"Сводка по возврату: "</strong>
                {format!(
                    "{} позиций, {} шт. всего, {:.2} сумма",
                    sorted_lines.len(),
                    total_items,
                    total_amount,
                )}

            </div>

            <div class="table-container">
                <table class="table__data" id=TABLE_ID_LINES>
                    <thead>
                        <tr>
                            <th class="resizable" on:click=move |_| handle_sort("shop_sku")>
                                <span class="table__sortable-header">
                                    "Shop SKU"
                                    <span class=move || {
                                        get_sort_class(sort_column.get().unwrap_or(""), "shop_sku")
                                    }>

                                        {move || {
                                            get_sort_indicator(
                                                sort_column.get().unwrap_or(""),
                                                "shop_sku",
                                                sort_asc.get(),
                                            )
                                        }}

                                    </span>
                                </span>
                            </th>
                            <th class="resizable" on:click=move |_| handle_sort("name")>
                                <span class="table__sortable-header">
                                    "Наименование"
                                    <span class=move || {
                                        get_sort_class(sort_column.get().unwrap_or(""), "name")
                                    }>

                                        {move || {
                                            get_sort_indicator(
                                                sort_column.get().unwrap_or(""),
                                                "name",
                                                sort_asc.get(),
                                            )
                                        }}

                                    </span>
                                </span>
                            </th>
                            <th class="resizable text-right" on:click=move |_| handle_sort("count")>
                                <span class="table__sortable-header">
                                    "Кол-во"
                                    <span class=move || {
                                        get_sort_class(sort_column.get().unwrap_or(""), "count")
                                    }>

                                        {move || {
                                            get_sort_indicator(
                                                sort_column.get().unwrap_or(""),
                                                "count",
                                                sort_asc.get(),
                                            )
                                        }}

                                    </span>
                                </span>
                            </th>
                            <th class="resizable text-right" on:click=move |_| handle_sort("price")>
                                <span class="table__sortable-header">
                                    "Цена"
                                    <span class=move || {
                                        get_sort_class(sort_column.get().unwrap_or(""), "price")
                                    }>

                                        {move || {
                                            get_sort_indicator(
                                                sort_column.get().unwrap_or(""),
                                                "price",
                                                sort_asc.get(),
                                            )
                                        }}

                                    </span>
                                </span>
                            </th>
                            <th class="resizable">"Причина"</th>
                            <th class="resizable">"Тип решения"</th>
                            <th class="resizable">"Комментарий"</th>
                        </tr>
                    </thead>
                    <tbody>
                        {sorted_lines
                            .iter()
                            .map(|line| {
                                let decision_type = line
                                    .decisions
                                    .first()
                                    .map(|d| d.decision_type.clone())
                                    .unwrap_or("—".to_string());
                                let comment = line
                                    .decisions
                                    .first()
                                    .and_then(|d| d.comment.clone())
                                    .unwrap_or("—".to_string());
                                view! {
                                    <tr>
                                        <td>
                                            <code style="font-size: var(--font-size-xs);">
                                                {line.shop_sku.clone()}
                                            </code>
                                        </td>
                                        <td>{line.name.clone()}</td>
                                        <td class="text-right">
                                            <strong>{line.count}</strong>
                                        </td>
                                        <td class="text-right">
                                            {line.price.map(|p| format!("{:.2}", p)).unwrap_or("—".to_string())}
                                        </td>
                                        <td style="font-size: var(--font-size-xs);">
                                            {line.return_reason.clone().unwrap_or("—".to_string())}
                                        </td>
                                        <td style="font-size: var(--font-size-xs);">
                                            {decision_type}
                                        </td>
                                        <td style="font-size: var(--font-size-xs);">
                                            {comment}
                                        </td>
                                    </tr>
                                }
                            })
                            .collect_view()}

                        <tr style="background: var(--color-bg-secondary); font-weight: var(--font-weight-semibold);">
                            <td colspan="2" class="text-right">"Итого:"</td>
                            <td class="text-right">{total_items}</td>
                            <td class="text-right" style="color: #c62828;">
                                {format!("{:.2}", total_amount)}
                            </td>
                            <td colspan="3"></td>
                        </tr>
                    </tbody>
                </table>
            </div>
        </div>
    }
}
