//! Sales tab - linked WB sales by document_no

use super::super::api::WbSalesListItemDto;
use super::super::view_model::WbOrdersDetailsVm;
use crate::layout::global_context::AppGlobalContext;
use crate::shared::components::card_animated::CardAnimated;
use crate::shared::components::table::TableCellMoney;
use leptos::prelude::*;
use thaw::*;

fn format_iso_date(iso: &str) -> String {
    let date_part = iso.split('T').next().unwrap_or(iso);
    let mut parts = date_part.split('-');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(year), Some(month), Some(day)) => format!("{}.{}.{}", day, month, year),
        _ => iso.to_string(),
    }
}

#[component]
pub fn SalesTab(vm: WbOrdersDetailsVm) -> impl IntoView {
    let tabs_store =
        leptos::context::use_context::<AppGlobalContext>().expect("AppGlobalContext not found");

    view! {
        {move || {
            if vm.wb_sales_loading.get() {
                return view! {
                    <CardAnimated delay_ms=0 nav_id="a015_wb_orders_details_sales_loading">
                        <Flex gap=FlexGap::Small style="align-items: center; justify-content: center; padding: var(--spacing-xl);">
                            <Spinner />
                            <span>"Загрузка WB Sales..."</span>
                        </Flex>
                    </CardAnimated>
                }
                .into_any();
            }

            if let Some(err) = vm.wb_sales_error.get() {
                return view! {
                    <CardAnimated delay_ms=0 nav_id="a015_wb_orders_details_sales_error">
                        <div style="color: var(--color-error);">
                            "Ошибка загрузки: " {err}
                        </div>
                    </CardAnimated>
                }
                .into_any();
            }

            let sales = vm.wb_sales.get();
            if sales.is_empty() {
                return view! {
                    <CardAnimated delay_ms=0 nav_id="a015_wb_orders_details_sales_empty">
                        <h4 class="details-section__title">"WB Sales"</h4>
                        <div style="color: var(--color-text-secondary);">
                            "Связанные продажи не найдены для данного SRID."
                        </div>
                    </CardAnimated>
                }
                .into_any();
            }

            let sales_count = sales.len();
            let total_qty: f64 = sales.iter().map(|s| s.line.qty).sum();
            let total_amount: f64 = sales.iter().filter_map(|s| s.line.finished_price).sum();
            let sales_for_table = sales;

            view! {
                <CardAnimated delay_ms=0 nav_id="a015_wb_orders_details_sales_main">
                    <h4 class="details-section__title">"WB Sales"</h4>

                    <Flex gap=FlexGap::Medium style="flex-wrap: wrap; margin-bottom: var(--spacing-md);">
                        <Badge appearance=BadgeAppearance::Tint color=BadgeColor::Brand>
                            {format!("Найдено: {}", sales_count)}
                        </Badge>
                        <span>"Qty: " <strong>{format!("{:.2}", total_qty)}</strong></span>
                        <span>"Сумма: " <strong>{format!("{:.2}", total_amount)}</strong></span>
                    </Flex>

                    <div style="max-height: calc(100vh - 400px); overflow: auto;">
                        <Table>
                            <TableHeader>
                                <TableRow>
                                    <TableHeaderCell>"Дата продажи"</TableHeaderCell>
                                    <TableHeaderCell>"Артикул поставщика"</TableHeaderCell>
                                    <TableHeaderCell>"Количество"</TableHeaderCell>
                                    <TableHeaderCell>"Сумма"</TableHeaderCell>
                                    <TableHeaderCell>"Тип события"</TableHeaderCell>
                                </TableRow>
                            </TableHeader>
                            <TableBody>
                                <For
                                    each=move || sales_for_table.clone()
                                    key=|s| s.id.clone()
                                    children={
                                        let tabs_store = tabs_store;
                                        move |sale: WbSalesListItemDto| {
                                            let sale_id = sale.id.clone();
                                            let sale_doc_no = sale.header.document_no.clone();
                                            let event_type = sale.state.event_type.clone();
                                            let event_type_lower = event_type.to_lowercase();
                                            let event_badge_color = if event_type_lower.contains("sale")
                                                || event_type_lower.contains("прод")
                                            {
                                                BadgeColor::Success
                                            } else if event_type_lower.contains("return")
                                                || event_type_lower.contains("возв")
                                            {
                                                BadgeColor::Danger
                                            } else {
                                                BadgeColor::Informative
                                            };

                                            view! {
                                                <TableRow
                                                    on:click={
                                                        let tabs_store = tabs_store;
                                                        move |_| {
                                                            let tab_key = format!("a012_wb_sales_details_{}", sale_id);
                                                            let tab_title = format!("WB Sale {}", sale_doc_no);
                                                            tabs_store.open_tab(&tab_key, &tab_title);
                                                        }
                                                    }
                                                    attr:style="cursor: pointer;"
                                                >
                                                    <TableCell><TableCellLayout>{format_iso_date(&sale.state.sale_dt)}</TableCellLayout></TableCell>
                                                    <TableCell><TableCellLayout>{sale.line.supplier_article}</TableCellLayout></TableCell>
                                                    <TableCell><TableCellLayout>{format!("{:.2}", sale.line.qty)}</TableCellLayout></TableCell>
                                                    <TableCellMoney value=sale.line.finished_price show_currency=false color_by_sign=false />
                                                    <TableCell>
                                                        <TableCellLayout>
                                                            <Badge appearance=BadgeAppearance::Tint color=event_badge_color>
                                                                {event_type}
                                                            </Badge>
                                                        </TableCellLayout>
                                                    </TableCell>
                                                </TableRow>
                                            }
                                        }
                                    }
                                />
                            </TableBody>
                        </Table>
                    </div>
                </CardAnimated>
            }
            .into_any()
        }}
    }
}
