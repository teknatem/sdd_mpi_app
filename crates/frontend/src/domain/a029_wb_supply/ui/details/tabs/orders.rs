//! Orders tab - shipment sheet

use super::super::api::{NomenclatureInfo, SupplyOrderDto};
use super::super::view_model::WbSupplyDetailsVm;
use crate::layout::global_context::AppGlobalContext;
use leptos::prelude::*;
use std::collections::HashMap;
use thaw::*;

const ZERO_UUID: &str = "00000000-0000-0000-0000-000000000000";

fn format_price_rub(price_kop: Option<i64>) -> String {
    match price_kop {
        Some(p) if p > 0 => format!("{:.2} ₽", p as f64 / 100.0),
        _ => "—".to_string(),
    }
}

fn format_date_short(iso: &str) -> String {
    if let Some(date_part) = iso.split('T').next() {
        if let Some((year, rest)) = date_part.split_once('-') {
            if let Some((month, day)) = rest.split_once('-') {
                return format!("{}.{}.{}", day, month, year);
            }
        }
    }
    iso.to_string()
}

fn effective_base_nomenclature_ref(order: &SupplyOrderDto) -> Option<String> {
    order
        .base_nomenclature_ref
        .as_deref()
        .map(str::trim)
        .filter(|base_ref| {
            !base_ref.is_empty()
                && *base_ref != ZERO_UUID
                && Some(*base_ref) != order.nomenclature_ref.as_deref()
        })
        .map(ToOwned::to_owned)
}

fn nomenclature_label(info_map: &HashMap<String, NomenclatureInfo>, nom_ref: &str) -> String {
    info_map
        .get(nom_ref)
        .map(|info| {
            if info.article.trim().is_empty() {
                info.description.clone()
            } else {
                format!("{} (арт. {})", info.description, info.article)
            }
        })
        .unwrap_or_else(|| nom_ref.to_string())
}

fn nomenclature_link(
    tabs_store: AppGlobalContext,
    nomenclatures_info: RwSignal<HashMap<String, NomenclatureInfo>>,
    nom_ref: Option<String>,
    tab_title: &'static str,
) -> impl IntoView {
    if let Some(nom_ref_value) = nom_ref {
        let nom_ref_for_click = nom_ref_value.clone();
        let title = nom_ref_value.clone();
        view! {
            <a
                href="#"
                title=title
                on:click=move |ev: web_sys::MouseEvent| {
                    ev.prevent_default();
                    tabs_store.open_tab(
                        &format!("a004_nomenclature_details_{}", nom_ref_for_click),
                        tab_title,
                    );
                }
                style="color: #0078d4; text-decoration: underline; cursor: pointer;"
            >
                {move || nomenclature_label(&nomenclatures_info.get(), &nom_ref_value)}
            </a>
        }
        .into_any()
    } else {
        view! { <span>"—"</span> }.into_any()
    }
}

#[component]
pub fn OrdersTab(vm: WbSupplyDetailsVm) -> impl IntoView {
    let tabs_store =
        leptos::context::use_context::<AppGlobalContext>().expect("AppGlobalContext not found");
    let nomenclatures_info = vm.nomenclatures_info;

    view! {
        <div style="width: 100%; min-width: 0;">
            <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 12px; flex-wrap: wrap; gap: 8px;">
                <div style="display: flex; align-items: center; gap: 10px;">
                    <h3 class="details-section__title" style="margin: 0;">"Заказы в поставке"</h3>
                    {move || {
                        let count = vm.orders.get().len();
                        if count > 0 {
                            view! { <span class="badge badge--primary">{count}</span> }.into_any()
                        } else {
                            view! { <></> }.into_any()
                        }
                    }}
                </div>
                <Button
                    appearance=ButtonAppearance::Secondary
                    size=ButtonSize::Small
                    on_click=move |_| {
                        if let Some(window) = web_sys::window() {
                            let _ = window.print();
                        }
                    }
                >
                    "🖨 Печать"
                </Button>
            </div>

            {move || {
                let orders = vm.orders.get();
                if orders.is_empty() {
                    return view! {
                        <div style="padding: 24px 0; color: var(--color-text-secondary); font-size: var(--font-size-sm);">
                            "Нет заказов в поставке. Загрузите оперативные заказы или выполните привязку к поставкам."
                        </div>
                    }
                    .into_any();
                }

                let total = orders.len();
                let total_price: f64 = orders.iter().filter_map(|o| o.price).map(|p| p as f64 / 100.0).sum();

                view! {
                    <div>
                        <div style="margin-bottom: 8px; color: var(--color-text-secondary); font-size: var(--font-size-sm); display: flex; gap: 16px;">
                            <span>{format!("Заказов: {}", total)}</span>
                            {if total_price > 0.0 {
                                view! { <span>{format!("Сумма: {:.2} ₽", total_price)}</span> }.into_any()
                            } else {
                                view! { <></> }.into_any()
                            }}
                        </div>
                        <div class="table-wrapper" style="width: 100%; min-width: 0; overflow-x: auto;">
                            <Table attr:id="a029-orders-table" attr:style="width: 100%; min-width: 1180px;">
                                <TableHeader>
                                    <TableRow>
                                        <TableHeaderCell resizable=true min_width=40.0>"#"</TableHeaderCell>
                                        <TableHeaderCell resizable=true min_width=130.0>"Артикул"</TableHeaderCell>
                                        <TableHeaderCell resizable=true min_width=80.0>"nmId"</TableHeaderCell>
                                        <TableHeaderCell resizable=true min_width=140.0>"Баркод"</TableHeaderCell>
                                        <TableHeaderCell resizable=true min_width=220.0>"Номенклатура 1С"</TableHeaderCell>
                                        <TableHeaderCell resizable=true min_width=220.0>"Базовая номенклатура"</TableHeaderCell>
                                        <TableHeaderCell resizable=true min_width=90.0>"Стикер"</TableHeaderCell>
                                        <TableHeaderCell resizable=true min_width=90.0>"Цена"</TableHeaderCell>
                                        <TableHeaderCell resizable=true min_width=80.0>"Дата"</TableHeaderCell>
                                        <TableHeaderCell resizable=true min_width=90.0>"Статус"</TableHeaderCell>
                                    </TableRow>
                                </TableHeader>
                                <TableBody>
                                    {orders
                                        .into_iter()
                                        .enumerate()
                                        .map(|(i, order)| {
                                            let article = order.article.clone().unwrap_or_else(|| "—".to_string());
                                            let nm_id = order.nm_id.map(|v| v.to_string()).unwrap_or_else(|| "—".to_string());
                                            let barcode = order.barcodes.first().cloned().unwrap_or_else(|| "—".to_string());
                                            let nomenclature_ref = order.nomenclature_ref.clone();
                                            let base_nomenclature_ref = effective_base_nomenclature_ref(&order);
                                            let sticker = match (order.part_a, order.part_b) {
                                                (Some(a), Some(b)) => format!("{}-{:04}", a, b),
                                                (Some(a), None) => a.to_string(),
                                                _ => order.color_code.clone().unwrap_or_else(|| "—".to_string()),
                                            };
                                            let price = format_price_rub(order.price);
                                            let date = order.created_at.as_deref().map(format_date_short).unwrap_or_else(|| "—".to_string());
                                            let is_cancel = order.status.as_deref().map(|s| s.starts_with("cancel")).unwrap_or(false);
                                            let status = match order.status.as_deref() {
                                                Some("cancel") | Some("cancelled") => "Отменён".to_string(),
                                                Some("cancelledByClient") => "Отменён кл.".to_string(),
                                                Some("sold") | Some("sorted") => "Продан".to_string(),
                                                Some(s) if !s.is_empty() => s.to_string(),
                                                _ => "—".to_string(),
                                            };
                                            let status_style = if is_cancel { "color: var(--color-error);" } else { "" };
                                            view! {
                                                <TableRow>
                                                    <TableCell>
                                                        <TableCellLayout>
                                                            <span style="color: var(--color-text-secondary); font-size: 0.8em;">{i + 1}</span>
                                                        </TableCellLayout>
                                                    </TableCell>
                                                    <TableCell>
                                                        <TableCellLayout truncate=true>
                                                            <strong>{article}</strong>
                                                        </TableCellLayout>
                                                    </TableCell>
                                                    <TableCell>
                                                        <TableCellLayout>
                                                            <code style="font-size: 0.82em;">{nm_id}</code>
                                                        </TableCellLayout>
                                                    </TableCell>
                                                    <TableCell>
                                                        <TableCellLayout truncate=true>
                                                            <code style="font-size: 0.82em;">{barcode}</code>
                                                        </TableCellLayout>
                                                    </TableCell>
                                                    <TableCell>
                                                        <TableCellLayout truncate=true>
                                                            {nomenclature_link(
                                                                tabs_store.clone(),
                                                                nomenclatures_info,
                                                                nomenclature_ref,
                                                                "Номенклатура",
                                                            )}
                                                        </TableCellLayout>
                                                    </TableCell>
                                                    <TableCell>
                                                        <TableCellLayout truncate=true>
                                                            {nomenclature_link(
                                                                tabs_store.clone(),
                                                                nomenclatures_info,
                                                                base_nomenclature_ref,
                                                                "Базовая номенклатура",
                                                            )}
                                                        </TableCellLayout>
                                                    </TableCell>
                                                    <TableCell>
                                                        <TableCellLayout>
                                                            <strong style="color: var(--color-accent);">{sticker}</strong>
                                                        </TableCellLayout>
                                                    </TableCell>
                                                    <TableCell>
                                                        <TableCellLayout>{price}</TableCellLayout>
                                                    </TableCell>
                                                    <TableCell>
                                                        <TableCellLayout>{date}</TableCellLayout>
                                                    </TableCell>
                                                    <TableCell>
                                                        <TableCellLayout>
                                                            <span style=status_style>{status}</span>
                                                        </TableCellLayout>
                                                    </TableCell>
                                                </TableRow>
                                            }
                                        })
                                        .collect::<Vec<_>>() }
                                </TableBody>
                            </Table>
                        </div>
                    </div>
                }
                .into_any()
            }}
        </div>
    }
}
