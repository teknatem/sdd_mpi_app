//! Lines tab ("Позиции") - per-product advert metrics with sorting and CSV export.

use super::super::api::{fmt_money, fmt_ratio, LINES_COLUMN_WIDTHS_KEY, LINES_TABLE_ID};
use super::super::view_model::WbAdvertDailyDetailsVm;
use crate::shared::components::card_animated::CardAnimated;
use crate::shared::components::table::TableCrosshairHighlight;
use crate::shared::export::export_to_excel;
use crate::shared::icons::icon;
use crate::shared::list_utils::{get_sort_class, get_sort_indicator};
use crate::shared::table_utils::{clear_resize_flag, init_column_resize, was_just_resizing};
use leptos::prelude::*;
use leptos::task::spawn_local;
use thaw::*;

#[component]
fn SortHeaderCell(
    label: &'static str,
    field: &'static str,
    min_width: f32,
    sort_field: RwSignal<String>,
    sort_ascending: RwSignal<bool>,
    on_toggle: Callback<&'static str>,
    #[prop(default = false)] numeric: bool,
) -> impl IntoView {
    let header_style = if numeric {
        "cursor: pointer; text-align: right; justify-content: flex-end;"
    } else {
        "cursor: pointer;"
    };

    view! {
        <TableHeaderCell resizable=false min_width=min_width class="resizable">
            <div class="table__sortable-header" style=header_style on:click=move |_| {
                if was_just_resizing() {
                    clear_resize_flag();
                    return;
                }
                on_toggle.run(field)
            }>
                {label}
                <span class=move || get_sort_class(&sort_field.get(), field)>
                    {move || get_sort_indicator(&sort_field.get(), field, sort_ascending.get())}
                </span>
            </div>
        </TableHeaderCell>
    }
}

#[component]
pub fn LinesTab(vm: WbAdvertDailyDetailsVm) -> impl IntoView {
    // Init column resize once the table is in the DOM.
    Effect::new(move |_| {
        spawn_local(async move {
            gloo_timers::future::TimeoutFuture::new(100).await;
            init_column_resize(LINES_TABLE_ID, LINES_COLUMN_WIDTHS_KEY);
        });
    });

    let doc = vm.doc;
    let sorted_lines = vm.sorted_lines();
    let sort_field = vm.lines_sort_field;
    let sort_ascending = vm.lines_sort_ascending;
    let error = vm.error;

    let toggle_sort = {
        let vm = vm.clone();
        Callback::new(move |field: &'static str| vm.toggle_lines_sort(field))
    };

    // Copy `Callback` for nmID → a007 navigation (usable inside `For` children).
    let open_product = Callback::new({
        let vm = vm.clone();
        move |product_ref: String| vm.open_product(product_ref)
    });

    move || {
        let Some(d) = doc.get() else {
            return view! { <div class="text-muted">"Нет данных"</div> }.into_any();
        };
        let total_lines = d.lines.len();
        let without_nomenclature = d
            .lines
            .iter()
            .filter(|line| line.nomenclature_ref.is_none())
            .count();

        let export_filename = format!(
            "wb_advert_daily_positions_{}_{}.csv",
            d.document_date, d.document_no
        );
        let export_lines = move |_| {
            let lines = sorted_lines.get_untracked();
            if let Err(err) = export_to_excel(&lines, &export_filename) {
                error.set(Some(format!("CSV: {}", err)));
            }
        };

        view! {
            <CardAnimated delay_ms=0 nav_id="a026_wb_advert_daily_details_lines_table">
                <div style="display:flex;gap:12px;flex-wrap:wrap;align-items:center;justify-content:space-between;">
                    <div style="display:flex;gap:12px;flex-wrap:wrap;align-items:center;">
                        <Badge appearance=BadgeAppearance::Tint color=BadgeColor::Informative>
                            {format!("Позиции: {}", total_lines)}
                        </Badge>
                        <Badge
                            appearance=BadgeAppearance::Tint
                            color={ if without_nomenclature > 0 { BadgeColor::Danger } else { BadgeColor::Success } }
                        >
                            {if without_nomenclature > 0 {
                                format!("Без номенклатуры: {}", without_nomenclature)
                            } else {
                                "Все позиции сопоставлены".to_string()
                            }}
                        </Badge>
                    </div>
                    <Button appearance=ButtonAppearance::Secondary on_click=export_lines>
                        {icon("download")}
                        "Excel (csv)"
                    </Button>
                </div>

                <div class="table-wrapper">
                    <TableCrosshairHighlight table_id=LINES_TABLE_ID.to_string() />
                    <Table attr:id=LINES_TABLE_ID attr:style="width:100%;min-width:1230px;">
                        <TableHeader>
                            <TableRow>
                                <SortHeaderCell label="nmID" field="nm_id" min_width=90.0 sort_field=sort_field sort_ascending=sort_ascending on_toggle=toggle_sort />
                                <SortHeaderCell label="WB наименование" field="wb_name" min_width=240.0 sort_field=sort_field sort_ascending=sort_ascending on_toggle=toggle_sort />
                                <SortHeaderCell label="Артикул 1С" field="nomenclature_article" min_width=140.0 sort_field=sort_field sort_ascending=sort_ascending on_toggle=toggle_sort />
                                <SortHeaderCell label="Просмотры" field="views" min_width=70.0 sort_field=sort_field sort_ascending=sort_ascending on_toggle=toggle_sort numeric=true />
                                <SortHeaderCell label="Клики" field="clicks" min_width=63.0 sort_field=sort_field sort_ascending=sort_ascending on_toggle=toggle_sort numeric=true />
                                <SortHeaderCell label="CTR, %" field="ctr" min_width=63.0 sort_field=sort_field sort_ascending=sort_ascending on_toggle=toggle_sort numeric=true />
                                <SortHeaderCell label="CPC" field="cpc" min_width=63.0 sort_field=sort_field sort_ascending=sort_ascending on_toggle=toggle_sort numeric=true />
                                <SortHeaderCell label="В корзину" field="atbs" min_width=77.0 sort_field=sort_field sort_ascending=sort_ascending on_toggle=toggle_sort numeric=true />
                                <SortHeaderCell label="Заказы" field="orders" min_width=63.0 sort_field=sort_field sort_ascending=sort_ascending on_toggle=toggle_sort numeric=true />
                                <SortHeaderCell label="Штуки" field="shks" min_width=77.0 sort_field=sort_field sort_ascending=sort_ascending on_toggle=toggle_sort numeric=true />
                                <SortHeaderCell label="Расход" field="sum" min_width=77.0 sort_field=sort_field sort_ascending=sort_ascending on_toggle=toggle_sort numeric=true />
                                <SortHeaderCell label="Выручка" field="sum_price" min_width=84.0 sort_field=sort_field sort_ascending=sort_ascending on_toggle=toggle_sort numeric=true />
                                <SortHeaderCell label="CR, %" field="cr" min_width=63.0 sort_field=sort_field sort_ascending=sort_ascending on_toggle=toggle_sort numeric=true />
                                <SortHeaderCell label="Отмены" field="canceled" min_width=63.0 sort_field=sort_field sort_ascending=sort_ascending on_toggle=toggle_sort numeric=true />
                            </TableRow>
                        </TableHeader>
                        <TableBody>
                            <For
                                each=move || sorted_lines.get()
                                key=|line| format!("{}:{}", line.nm_id, line.nomenclature_ref.clone().unwrap_or_default())
                                children=move |line| {
                                    let article = line.nomenclature_article.clone().unwrap_or_else(|| "—".to_string());
                                    let product_ref_val = line.marketplace_product_ref.clone().unwrap_or_default();
                                    view! {
                                        <TableRow>
                                            <TableCell><TableCellLayout>
                                                {if product_ref_val.is_empty() {
                                                    view! { <span>{line.nm_id}</span> }.into_any()
                                                } else {
                                                    view! {
                                                        <a href="#" class="table__link" on:click=move |e| {
                                                            e.prevent_default();
                                                            open_product.run(product_ref_val.clone());
                                                        }>{line.nm_id}</a>
                                                    }.into_any()
                                                }}
                                            </TableCellLayout></TableCell>
                                            <TableCell><TableCellLayout truncate=true>{line.wb_name}</TableCellLayout></TableCell>
                                            <TableCell><TableCellLayout truncate=true>{article}</TableCellLayout></TableCell>
                                            <TableCell class="text-right"><TableCellLayout>{line.metrics.views}</TableCellLayout></TableCell>
                                            <TableCell class="text-right"><TableCellLayout>{line.metrics.clicks}</TableCellLayout></TableCell>
                                            <TableCell class="text-right"><TableCellLayout>{fmt_ratio(line.metrics.ctr)}</TableCellLayout></TableCell>
                                            <TableCell class="text-right"><TableCellLayout>{fmt_money(line.metrics.cpc)}</TableCellLayout></TableCell>
                                            <TableCell class="text-right"><TableCellLayout>{line.metrics.atbs}</TableCellLayout></TableCell>
                                            <TableCell class="text-right"><TableCellLayout>{line.metrics.orders}</TableCellLayout></TableCell>
                                            <TableCell class="text-right"><TableCellLayout>{line.metrics.shks}</TableCellLayout></TableCell>
                                            <TableCell class="text-right"><TableCellLayout>{fmt_money(line.metrics.sum)}</TableCellLayout></TableCell>
                                            <TableCell class="text-right"><TableCellLayout>{fmt_money(line.metrics.sum_price)}</TableCellLayout></TableCell>
                                            <TableCell class="text-right"><TableCellLayout>{fmt_ratio(line.metrics.cr)}</TableCellLayout></TableCell>
                                            <TableCell class="text-right"><TableCellLayout>{line.metrics.canceled}</TableCellLayout></TableCell>
                                        </TableRow>
                                    }
                                }
                            />
                        </TableBody>
                    </Table>
                </div>
            </CardAnimated>
        }.into_any()
    }
}
