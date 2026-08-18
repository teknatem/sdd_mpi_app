//! Lines tab ("Позиции") - per-product stocks and ratings snapshot with sorting and CSV export.

use super::super::api::{fmt_money, fmt_ratio, LINES_COLUMN_WIDTHS_KEY, LINES_TABLE_ID};
use super::super::view_model::WbProductSnapshotDetailsVm;
use crate::shared::components::card_animated::CardAnimated;
use crate::shared::components::table::number_format::format_number_int;
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
    /// Фиксированная ширина колонки в ch (заголовок + ячейки), чтобы числовые
    /// колонки не растягивались — излишек уходит в текстовые колонки.
    #[prop(optional)]
    width_ch: Option<f32>,
) -> impl IntoView {
    let header_style = if numeric {
        "cursor: pointer; text-align: right; justify-content: flex-end;"
    } else {
        "cursor: pointer;"
    };
    let cell_style = width_ch
        .map(|w| format!("width: {w}ch; max-width: {w}ch;"))
        .unwrap_or_default();

    view! {
        <TableHeaderCell resizable=false min_width=min_width class="resizable" attr:style=cell_style>
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
pub fn LinesTab(vm: WbProductSnapshotDetailsVm) -> impl IntoView {
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

    // Воронка a036: метрики за N дней и элементы управления.
    let funnel_days = vm.funnel_days;
    let funnel_metrics = vm.funnel_metrics;
    let funnel_loading = vm.funnel_loading;
    let refresh_funnel = Callback::new({
        let vm = vm.clone();
        move |_: ()| vm.load_funnel()
    });

    let toggle_sort = {
        let vm = vm.clone();
        Callback::new(move |field: &'static str| vm.toggle_lines_sort(field))
    };

    let open_product = Callback::new({
        let vm = vm.clone();
        move |product_ref: String| vm.open_product(product_ref)
    });

    move || {
        let Some(d) = doc.get() else {
            return view! { <div class="text-muted">"Нет данных"</div> }.into_any();
        };
        let total_lines = d.lines.len();

        let export_filename = format!(
            "wb_product_snapshot_{}_{}.csv",
            d.document_date, d.document_no
        );
        let export_lines = move |_| {
            let lines = sorted_lines.get_untracked();
            if let Err(err) = export_to_excel(&lines, &export_filename) {
                error.set(Some(format!("CSV: {}", err)));
            }
        };

        view! {
            <CardAnimated delay_ms=0 nav_id="a037_wb_product_snapshot_details_lines_table">
                <div style="display:flex;gap:12px;flex-wrap:wrap;align-items:center;justify-content:space-between;">
                    <Badge appearance=BadgeAppearance::Tint color=BadgeColor::Informative>
                        {format!("Позиции: {}", total_lines)}
                    </Badge>
                    <div style="display:flex;gap:12px;flex-wrap:wrap;align-items:center;">
                        <div class="col-funnel-controls">
                            <span class="text-muted">"Воронка за (дней):"</span>
                            <input
                                type="number"
                                min="1"
                                max="90"
                                style="width:64px;"
                                prop:value=move || funnel_days.get().to_string()
                                on:change=move |ev| {
                                    if let Ok(n) = event_target_value(&ev).parse::<i64>() {
                                        funnel_days.set(n.max(1));
                                    }
                                }
                            />
                            <Button
                                appearance=ButtonAppearance::Secondary
                                on_click=move |_| refresh_funnel.run(())
                            >
                                "Обновить"
                            </Button>
                            {move || funnel_loading.get().then(|| view! { <span class="text-muted">"…"</span> })}
                        </div>
                        <Button appearance=ButtonAppearance::Secondary on_click=export_lines>
                            {icon("download")}
                            "Excel (csv)"
                        </Button>
                    </div>
                </div>

                <div class="table-wrapper">
                    <TableCrosshairHighlight table_id=LINES_TABLE_ID.to_string() />
                    <Table attr:id=LINES_TABLE_ID attr:style="width:100%;min-width:1200px;">
                        <TableHeader>
                            <TableRow>
                                <SortHeaderCell label="nmID" field="nm_id" min_width=90.0 sort_field=sort_field sort_ascending=sort_ascending on_toggle=toggle_sort />
                                <SortHeaderCell label="Наименование" field="title" min_width=240.0 sort_field=sort_field sort_ascending=sort_ascending on_toggle=toggle_sort />
                                <SortHeaderCell label="Артикул продавца" field="vendor_code" min_width=130.0 sort_field=sort_field sort_ascending=sort_ascending on_toggle=toggle_sort />
                                <SortHeaderCell label="Бренд" field="brand_name" min_width=110.0 sort_field=sort_field sort_ascending=sort_ascending on_toggle=toggle_sort />
                                <SortHeaderCell label="Артикул 1С" field="nomenclature_article" min_width=120.0 sort_field=sort_field sort_ascending=sort_ascending on_toggle=toggle_sort />
                                <TableHeaderCell resizable=false min_width=78.0 class="resizable col-funnel" attr:style="width:11ch;max-width:11ch;"><div style="text-align:right;justify-content:flex-end;width:100%;">"Переходы"</div></TableHeaderCell>
                                <TableHeaderCell resizable=false min_width=78.0 class="resizable col-funnel" attr:style="width:11ch;max-width:11ch;"><div style="text-align:right;justify-content:flex-end;width:100%;">"В корзину"</div></TableHeaderCell>
                                <TableHeaderCell resizable=false min_width=70.0 class="resizable col-funnel" attr:style="width:9ch;max-width:9ch;"><div style="text-align:right;justify-content:flex-end;width:100%;">"Заказы"</div></TableHeaderCell>
                                <SortHeaderCell label="Остаток WB" field="stock_wb" min_width=88.0 width_ch=12.0 sort_field=sort_field sort_ascending=sort_ascending on_toggle=toggle_sort numeric=true />
                                <SortHeaderCell label="Остаток продавца" field="stock_mp" min_width=118.0 width_ch=17.0 sort_field=sort_field sort_ascending=sort_ascending on_toggle=toggle_sort numeric=true />
                                <SortHeaderCell label="Сумма остатков" field="stock_balance_sum" min_width=104.0 width_ch=15.0 sort_field=sort_field sort_ascending=sort_ascending on_toggle=toggle_sort numeric=true />
                                <SortHeaderCell label="Рейтинг" field="product_rating" min_width=72.0 width_ch=10.0 sort_field=sort_field sort_ascending=sort_ascending on_toggle=toggle_sort numeric=true />
                                <SortHeaderCell label="Оценка" field="feedback_rating" min_width=72.0 width_ch=9.0 sort_field=sort_field sort_ascending=sort_ascending on_toggle=toggle_sort numeric=true />
                            </TableRow>
                        </TableHeader>
                        <TableBody>
                            <For
                                each=move || sorted_lines.get()
                                key=|line| line.nm_id
                                children=move |line| {
                                    let article = line.nomenclature_article.clone().unwrap_or_else(|| "—".to_string());
                                    let product_ref_val = line.marketplace_product_ref.clone().unwrap_or_default();
                                    let nm_id = line.nm_id;
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
                                            <TableCell><TableCellLayout truncate=true>{line.title}</TableCellLayout></TableCell>
                                            <TableCell><TableCellLayout truncate=true>{line.vendor_code}</TableCellLayout></TableCell>
                                            <TableCell><TableCellLayout truncate=true>{line.brand_name}</TableCellLayout></TableCell>
                                            <TableCell><TableCellLayout truncate=true>{article}</TableCellLayout></TableCell>
                                            <TableCell class="text-right col-funnel" attr:style="width:11ch;max-width:11ch;"><span>
                                                {move || format_number_int(funnel_metrics.with(|m| m.get(&nm_id).map(|v| v.0).unwrap_or(0)) as f64)}
                                            </span></TableCell>
                                            <TableCell class="text-right col-funnel" attr:style="width:11ch;max-width:11ch;"><span>
                                                {move || format_number_int(funnel_metrics.with(|m| m.get(&nm_id).map(|v| v.1).unwrap_or(0)) as f64)}
                                            </span></TableCell>
                                            <TableCell class="text-right col-funnel" attr:style="width:9ch;max-width:9ch;"><span>
                                                {move || format_number_int(funnel_metrics.with(|m| m.get(&nm_id).map(|v| v.2).unwrap_or(0)) as f64)}
                                            </span></TableCell>
                                            <TableCell class="text-right" attr:style="width:12ch;max-width:12ch;"><span>{line.state.stock_wb}</span></TableCell>
                                            <TableCell class="text-right" attr:style="width:17ch;max-width:17ch;"><span>{line.state.stock_mp}</span></TableCell>
                                            <TableCell class="text-right" attr:style="width:15ch;max-width:15ch;"><span>{fmt_money(line.state.stock_balance_sum)}</span></TableCell>
                                            <TableCell class="text-right" attr:style="width:10ch;max-width:10ch;"><span>{fmt_ratio(line.state.product_rating)}</span></TableCell>
                                            <TableCell class="text-right" attr:style="width:9ch;max-width:9ch;"><span>{fmt_ratio(line.state.feedback_rating)}</span></TableCell>
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
