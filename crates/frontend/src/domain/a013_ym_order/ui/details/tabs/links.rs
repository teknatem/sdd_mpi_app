//! Links tab - YM Payment Report records linked by order_id

use super::super::model::YmPaymentReportLinkDto;
use super::super::view_model::YmOrderDetailsVm;
use crate::layout::global_context::AppGlobalContext;
use crate::shared::components::card_animated::CardAnimated;
use crate::shared::components::table::TableCellMoney;
use crate::shared::icons::icon;
use leptos::prelude::*;
use thaw::*;

fn fmt_date(s: &str) -> String {
    let bytes = s.as_bytes();
    if bytes.len() >= 10 && bytes[4] == b'-' && bytes[7] == b'-' {
        let year = &s[0..4];
        let month = &s[5..7];
        let day = &s[8..10];
        let rest = &s[10..];
        return format!("{}.{}.{}{}", day, month, year, rest);
    }
    s.to_string()
}

/// Справочные строки p907 (`payment_status` вида «Справочно: …») дублируют суммы
/// уже учтённых транзакций и исключаются из итога по колонке «Сумма».
fn is_reference_row(payment_status: Option<&str>) -> bool {
    payment_status
        .map(|s| s.trim_start().starts_with("Справочно"))
        .unwrap_or(false)
}

/// TSV-представление таблицы для копирования в буфер обмена (вставляется в Excel).
fn build_copy_tsv(reports: &[YmPaymentReportLinkDto], total_counted_sum: f64) -> String {
    let money = |v: Option<f64>| v.map(|n| format!("{:.2}", n)).unwrap_or_default();
    let mut out = String::from(
        "Дата\tТип транзакции\tSKU\tТовар / Услуга\tКол-во\tСумма\tСумма (в итоге)\tСумма ПП\tСтатус\tИсточник\n",
    );
    for r in reports {
        let date = r
            .transaction_date
            .as_deref()
            .map(fmt_date)
            .unwrap_or_default();
        let counted = if is_reference_row(r.payment_status.as_deref()) {
            String::new()
        } else {
            money(r.transaction_sum)
        };
        out.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            date,
            r.transaction_type.clone().unwrap_or_default(),
            r.shop_sku.clone().unwrap_or_default(),
            r.offer_or_service_name.clone().unwrap_or_default(),
            r.count.map(|v| v.to_string()).unwrap_or_default(),
            money(r.transaction_sum),
            counted,
            money(r.bank_sum),
            r.payment_status.clone().unwrap_or_default(),
            r.transaction_source.clone().unwrap_or_default(),
        ));
    }
    out.push_str(&format!(
        "Итого\t\t\t\t\t\t{:.2}\t\t\t\n",
        total_counted_sum
    ));
    out
}

/// Links tab: displays p907_ym_payment_report rows for this order
#[component]
pub fn LinksTab(vm: YmOrderDetailsVm) -> impl IntoView {
    let tabs_store =
        leptos::context::use_context::<AppGlobalContext>().expect("AppGlobalContext not found");

    view! {
        {move || {
            if vm.payment_reports_loading.get() {
                return view! {
                    <CardAnimated delay_ms=0 nav_id="a013_ym_order_details_links_loading">
                        <Flex gap=FlexGap::Small style="align-items: center; justify-content: center; padding: var(--spacing-xl);">
                            <Spinner />
                            <span>"Загрузка платёжных отчётов..."</span>
                        </Flex>
                    </CardAnimated>
                }.into_any();
            }

            if let Some(err) = vm.payment_reports_error.get() {
                return view! {
                    <CardAnimated delay_ms=0 nav_id="a013_ym_order_details_links_error">
                        <div style="color: var(--color-error);">
                            "Ошибка загрузки: " {err}
                        </div>
                    </CardAnimated>
                }.into_any();
            }

            let reports = vm.payment_reports.get();
            if reports.is_empty() {
                return view! {
                    <CardAnimated delay_ms=0 nav_id="a013_ym_order_details_links_empty">
                        <h4 class="details-section__title">"Платёжные отчёты (p907)"</h4>
                        <div style="color: var(--color-text-secondary);">
                            "Связанные платёжные транзакции для данного заказа не найдены."
                        </div>
                    </CardAnimated>
                }.into_any();
            }

            let reports_count = reports.len();
            // Итог по «Сумме» — только по учтённым строкам (без «Справочно: …»).
            let total_counted_sum: f64 = reports
                .iter()
                .filter(|r| !is_reference_row(r.payment_status.as_deref()))
                .filter_map(|r| r.transaction_sum)
                .sum();
            let copy_tsv = StoredValue::new(build_copy_tsv(&reports, total_counted_sum));
            let reports_for_table = reports;

            let copy_to_clipboard = move |_| {
                let text = copy_tsv.get_value();
                wasm_bindgen_futures::spawn_local(async move {
                    if let Some(window) = web_sys::window() {
                        let _ = window.navigator().clipboard().write_text(&text);
                    }
                });
            };

            view! {
                <CardAnimated delay_ms=0 nav_id="a013_ym_order_details_links_main">
                    <h4 class="details-section__title">"Платёжные отчёты (p907)"</h4>

                    <Flex gap=FlexGap::Medium style="flex-wrap: wrap; align-items: center; margin-bottom: var(--spacing-md);">
                        <Badge appearance=BadgeAppearance::Tint color=BadgeColor::Brand>
                            {format!("Найдено: {}", reports_count)}
                        </Badge>
                        <span>
                            "Сумма (без справочных): "
                            <strong>{format!("{:.2}", total_counted_sum)}</strong>
                        </span>
                        <Button
                            appearance=ButtonAppearance::Secondary
                            size=ButtonSize::Small
                            on_click=copy_to_clipboard
                        >
                            {icon("copy")} " Копировать таблицу"
                        </Button>
                    </Flex>

                    <div style="max-height: calc(100vh - 400px); overflow-y: auto; overflow-x: hidden;">
                        <Table attr:style="width: 100%;">
                            <TableHeader>
                                <TableRow>
                                    <TableHeaderCell resizable=true min_width=120.0>"Дата"</TableHeaderCell>
                                    <TableHeaderCell resizable=true min_width=140.0>"Тип транзакции"</TableHeaderCell>
                                    <TableHeaderCell resizable=true min_width=120.0>"SKU"</TableHeaderCell>
                                    <TableHeaderCell resizable=true min_width=200.0>"Товар / Услуга"</TableHeaderCell>
                                    <TableHeaderCell resizable=true min_width=60.0>"Кол-во"</TableHeaderCell>
                                    <TableHeaderCell resizable=true min_width=100.0>"Сумма"</TableHeaderCell>
                                    <TableHeaderCell resizable=true min_width=110.0>"Сумма (в итоге)"</TableHeaderCell>
                                    <TableHeaderCell resizable=true min_width=100.0>"Сумма ПП"</TableHeaderCell>
                                    <TableHeaderCell resizable=true min_width=100.0>"Статус"</TableHeaderCell>
                                    <TableHeaderCell resizable=true min_width=100.0>"Источник"</TableHeaderCell>
                                </TableRow>
                            </TableHeader>
                            <TableBody>
                                <For
                                    each=move || reports_for_table.clone()
                                    key=|r| r.id.clone()
                                    children={
                                        let tabs_store = tabs_store;
                                        move |report| {
                                            let row_id = report.id.clone();
                                            let date_str = report.transaction_date.as_deref().map(fmt_date).unwrap_or_default();
                                            let date_for_title = date_str.clone();
                                            // Учтённая сумма: пусто для справочных строк.
                                            let is_reference = is_reference_row(report.payment_status.as_deref());
                                            let counted_sum = if is_reference { None } else { report.transaction_sum };

                                            view! {
                                                <TableRow
                                                    on:click={
                                                        let tabs_store = tabs_store;
                                                        move |_| {
                                                            // UUID is URL-safe — no encoding needed.
                                                            let tab_key = format!(
                                                                "p907_ym_payment_report_details_{}",
                                                                row_id
                                                            );
                                                            let tab_title = format!("YM Платёж {}", date_for_title);
                                                            tabs_store.open_tab(&tab_key, &tab_title);
                                                        }
                                                    }
                                                    attr:style="cursor: pointer;"
                                                >
                                                    <TableCell>
                                                        <TableCellLayout>{date_str}</TableCellLayout>
                                                    </TableCell>
                                                    <TableCell>
                                                        <TableCellLayout truncate=true>
                                                            {report.transaction_type.unwrap_or_else(|| "—".to_string())}
                                                        </TableCellLayout>
                                                    </TableCell>
                                                    <TableCell>
                                                        <TableCellLayout truncate=true>
                                                            {report.shop_sku.unwrap_or_else(|| "—".to_string())}
                                                        </TableCellLayout>
                                                    </TableCell>
                                                    <TableCell>
                                                        <TableCellLayout truncate=true>
                                                            {report.offer_or_service_name.unwrap_or_else(|| "—".to_string())}
                                                        </TableCellLayout>
                                                    </TableCell>
                                                    <TableCell>
                                                        <TableCellLayout>
                                                            {report.count.map(|v| v.to_string()).unwrap_or_else(|| "—".to_string())}
                                                        </TableCellLayout>
                                                    </TableCell>
                                                    <TableCellMoney value=report.transaction_sum />
                                                    <TableCellMoney value=counted_sum />
                                                    <TableCellMoney value=report.bank_sum />
                                                    <TableCell>
                                                        <TableCellLayout truncate=true>
                                                            {report.payment_status.unwrap_or_else(|| "—".to_string())}
                                                        </TableCellLayout>
                                                    </TableCell>
                                                    <TableCell>
                                                        <TableCellLayout truncate=true>
                                                            {report.transaction_source.unwrap_or_else(|| "—".to_string())}
                                                        </TableCellLayout>
                                                    </TableCell>
                                                </TableRow>
                                            }
                                        }
                                    }
                                />
                                <TableRow attr:style="font-weight: 600; border-top: 2px solid var(--color-border);">
                                    <TableCell>
                                        <TableCellLayout>"Итого"</TableCellLayout>
                                    </TableCell>
                                    <TableCell></TableCell>
                                    <TableCell></TableCell>
                                    <TableCell></TableCell>
                                    <TableCell></TableCell>
                                    <TableCell></TableCell>
                                    <TableCellMoney value=Signal::derive(move || Some(total_counted_sum)) bold=true color_by_sign=false />
                                    <TableCell></TableCell>
                                    <TableCell></TableCell>
                                    <TableCell></TableCell>
                                </TableRow>
                            </TableBody>
                        </Table>
                    </div>
                </CardAnimated>
            }.into_any()
        }}
    }
}
