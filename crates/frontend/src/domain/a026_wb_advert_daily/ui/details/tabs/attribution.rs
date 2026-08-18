//! Attribution tab ("Атрибуция") - found a015 orders per nm, with cost allocation tree.

use super::super::api::{
    fmt_date, fmt_expense_share, fmt_money, should_show_linked_group, should_show_linked_order,
    LINKED_ORDERS_COLUMN_WIDTHS_KEY, LINKED_ORDERS_TABLE_ID,
};
use super::super::view_model::WbAdvertDailyDetailsVm;
use crate::shared::components::card_animated::CardAnimated;
use crate::shared::icons::icon;
use crate::shared::table_utils::init_column_resize;
use leptos::prelude::*;
use leptos::task::spawn_local;
use thaw::*;

#[component]
pub fn AttributionTab(vm: WbAdvertDailyDetailsVm) -> impl IntoView {
    // Init column resize once the table is in the DOM.
    Effect::new(move |_| {
        spawn_local(async move {
            gloo_timers::future::TimeoutFuture::new(100).await;
            init_column_resize(LINKED_ORDERS_TABLE_ID, LINKED_ORDERS_COLUMN_WIDTHS_KEY);
        });
    });

    let doc = vm.doc;
    let tree_expanded = vm.linked_orders_tree_expanded;
    let set_tree_expanded = vm.linked_orders_tree_expanded;

    // Copy `Callback`s for navigation — usable inside nested `For` children closures
    // without making them `FnOnce`.
    let open_order = Callback::new({
        let vm = vm.clone();
        move |order_id: String| vm.open_order(order_id)
    });
    let open_nom = Callback::new({
        let vm = vm.clone();
        move |nom_ref: String| vm.open_nomenclature(nom_ref)
    });

    move || {
        let Some(d) = doc.get() else {
            return view! { <div class="text-muted">"Нет данных"</div> }.into_any();
        };
        let has_linked = d.has_linked_orders;
        let count = d.linked_orders_count;
        let wb_orders_total: i64 = d.linked_orders.iter().map(|g| g.wb_reported_orders).sum();
        let groups: Vec<_> = d
            .linked_orders
            .iter()
            .filter(|group| should_show_linked_group(group))
            .cloned()
            .collect();
        let total_expense = d.totals.sum;
        let posted = d.is_posted;

        view! {
            <div style="display:flex;flex-direction:column;gap:var(--spacing-md);width:100%;">
                <CardAnimated delay_ms=0 nav_id="a026_wb_advert_daily_details_linked_orders_summary">
                    <h4 class="details-section__title">"Сводка"</h4>
                    <div style="display:flex;gap:12px;flex-wrap:wrap;align-items:center;">
                        <Badge
                            appearance=BadgeAppearance::Tint
                            color={ if has_linked { BadgeColor::Success } else if posted { BadgeColor::Warning } else { BadgeColor::Informative } }
                        >
                            { if !posted {
                                "Документ не проведён".to_string()
                            } else if has_linked {
                                "Найдены связанные заказы".to_string()
                            } else {
                                "Связанные заказы не найдены".to_string()
                            } }
                        </Badge>
                        <Badge appearance=BadgeAppearance::Tint color=BadgeColor::Informative>
                            {format!("Найдено: {}", count)}
                        </Badge>
                        <Badge appearance=BadgeAppearance::Tint color=BadgeColor::Brand>
                            {format!("По данным WB: {}", wb_orders_total)}
                        </Badge>
                    </div>
                    <Show when=move || !posted>
                        <div class="form__hint">
                            "Поиск связанных заказов выполняется при проведении документа. Проведите документ, чтобы увидеть результат."
                        </div>
                    </Show>
                    <Show when=move || posted && !has_linked>
                        <div class="form__hint">
                            "По строкам отчёта WB с заказами (orders > 0) нет данных для отображения."
                        </div>
                    </Show>
                    <Show when=move || posted && has_linked && count < wb_orders_total>
                        <div class="form__hint">
                            "Часть заказов WB не сопоставлена с a015 за дату документа — см. позиции со статусом «Нет в a015»."
                        </div>
                    </Show>
                </CardAnimated>
                <CardAnimated delay_ms=40 nav_id="a026_wb_advert_daily_details_linked_orders_table">
                    <div style="display:flex;align-items:center;justify-content:space-between;gap:var(--spacing-sm);flex-wrap:wrap;margin-bottom:var(--spacing-xs);">
                        <h4 class="details-section__title" style="margin:0;">"Найденные заказы по позициям"</h4>
                        <div style="display:flex;gap:var(--spacing-xs);align-items:center;">
                            <Button
                                size=ButtonSize::Small
                                appearance=Signal::derive(move || {
                                    if tree_expanded.get() { ButtonAppearance::Primary } else { ButtonAppearance::Subtle }
                                })
                                attr:title="Развернуть все уровни"
                                on_click=move |_| set_tree_expanded.set(true)
                            >
                                {icon("chevron-down")}
                            </Button>
                            <Button
                                size=ButtonSize::Small
                                appearance=Signal::derive(move || {
                                    if tree_expanded.get() { ButtonAppearance::Subtle } else { ButtonAppearance::Primary }
                                })
                                attr:title="Только 1 уровень"
                                on_click=move |_| set_tree_expanded.set(false)
                            >
                                {icon("chevron-right")}
                            </Button>
                        </div>
                    </div>
                    {
                        if groups.is_empty() {
                            view! {
                                <div class="text-muted">"Нет данных для отображения."</div>
                            }.into_any()
                        } else {
                            let groups_for_each = groups.clone();
                            view! {
                                <div class="table-wrapper" style="overflow-x: auto;">
                                    <Table attr:id=LINKED_ORDERS_TABLE_ID attr:style="width:100%;min-width:870px;table-layout:fixed;">
                                        <TableHeader>
                                            <TableRow>
                                                <TableHeaderCell resizable=false min_width=60.0 class="resizable" attr:style="width:250px;">"Наименование"</TableHeaderCell>
                                                <TableHeaderCell resizable=false min_width=60.0 class="resizable" attr:style="width:160px;">"Артикул / Заказ"</TableHeaderCell>
                                                <TableHeaderCell resizable=false min_width=50.0 class="resizable text-right" attr:style="width:65px;">"WB"</TableHeaderCell>
                                                <TableHeaderCell resizable=false min_width=50.0 class="resizable text-right" attr:style="width:75px;">"Найдено"</TableHeaderCell>
                                                <TableHeaderCell resizable=false min_width=50.0 class="resizable" attr:style="width:85px;">"Статус"</TableHeaderCell>
                                                <TableHeaderCell resizable=false min_width=50.0 class="resizable text-right" attr:style="width:80px;">"Цена"</TableHeaderCell>
                                                <TableHeaderCell resizable=false min_width=50.0 class="resizable text-right" attr:style="width:90px;">"Расход"</TableHeaderCell>
                                                <TableHeaderCell resizable=false min_width=50.0 class="resizable text-right" attr:style="width:65px;">"Доля, %"</TableHeaderCell>
                                            </TableRow>
                                        </TableHeader>
                                        <TableBody>
                                            <For
                                                each=move || groups_for_each.clone()
                                                key=|group| group.nm_id
                                                children=move |group| {
                                                    let header_name = group.nm_name.clone();
                                                    let wb_reported = group.wb_reported_orders;
                                                    let wb_advert_sum = group.wb_advert_sum;
                                                    let found_count = group.found_orders.len() as i64;
                                                    let allocated_count = group.found_orders.iter().filter(|o| o.is_allocated).count() as i64;
                                                    let extra_count = found_count - allocated_count;
                                                    let missing_a015 = wb_reported > 0 && found_count == 0;
                                                    let header_summary = if missing_a015 {
                                                        format!("0 / {wb_reported}")
                                                    } else if extra_count > 0 {
                                                        format!("{} (+{})", allocated_count, extra_count)
                                                    } else {
                                                        allocated_count.to_string()
                                                    };
                                                    let orders_for_each: Vec<_> = group
                                                        .found_orders
                                                        .iter()
                                                        .filter(|order| should_show_linked_order(order))
                                                        .cloned()
                                                        .collect();
                                                    let orders_stored = StoredValue::new(orders_for_each);
                                                    let group_share = fmt_expense_share(wb_advert_sum, total_expense);
                                                    let nom_ref_val = group.nomenclature_ref.clone().unwrap_or_default();
                                                    let article_text = group.nomenclature_article.clone().unwrap_or_else(|| "—".to_string());

                                                    view! {
                                                        // ── nm-group header row ──────────────────────────────────────────
                                                        <TableRow>
                                                            <TableCell><TableCellLayout truncate=true><strong>{header_name}</strong></TableCellLayout></TableCell>
                                                            <TableCell><TableCellLayout truncate=true>
                                                                <strong>
                                                                {if nom_ref_val.is_empty() {
                                                                    view! { <span>{article_text}</span> }.into_any()
                                                                } else {
                                                                    let nom_ref_click = nom_ref_val.clone();
                                                                    view! {
                                                                        <a href="#" class="table__link" on:click=move |e| {
                                                                            e.prevent_default();
                                                                            open_nom.run(nom_ref_click.clone());
                                                                        }>{article_text}</a>
                                                                    }.into_any()
                                                                }}
                                                                </strong>
                                                            </TableCellLayout></TableCell>
                                                            <TableCell class="text-right"><TableCellLayout>{wb_reported}</TableCellLayout></TableCell>
                                                            <TableCell class="text-right"><TableCellLayout>{header_summary}</TableCellLayout></TableCell>
                                                            <TableCell>
                                                                <TableCellLayout truncate=true>
                                                                    {if missing_a015 {
                                                                        view! {
                                                                            <Badge appearance=BadgeAppearance::Tint color=BadgeColor::Warning>
                                                                                "Нет в a015"
                                                                            </Badge>
                                                                        }.into_any()
                                                                    } else {
                                                                        view! { <span>""</span> }.into_any()
                                                                    }}
                                                                </TableCellLayout>
                                                            </TableCell>
                                                            <TableCell class="text-right"><TableCellLayout>"—"</TableCellLayout></TableCell>
                                                            <TableCell class="text-right"><TableCellLayout><strong>{fmt_money(wb_advert_sum)}</strong></TableCellLayout></TableCell>
                                                            <TableCell class="text-right"><TableCellLayout><strong>{group_share}</strong></TableCellLayout></TableCell>
                                                        </TableRow>
                                                        // ── per-order rows ───────────────────────────────────────────────
                                                        <Show when=move || tree_expanded.get()>
                                                        <Show when=move || missing_a015>
                                                            <TableRow>
                                                                <TableCell><TableCellLayout truncate=true>
                                                                    <span class="table__tree-child-marker" aria-hidden="true">"└─ "</span>
                                                                    "Нет проведённых заказов a015"
                                                                </TableCellLayout></TableCell>
                                                                <TableCell><TableCellLayout truncate=true>
                                                                    <span class="text-muted">"за дату документа"</span>
                                                                </TableCellLayout></TableCell>
                                                                <TableCell class="text-right"><TableCellLayout>""</TableCellLayout></TableCell>
                                                                <TableCell class="text-right"><TableCellLayout>""</TableCellLayout></TableCell>
                                                                <TableCell><TableCellLayout truncate=true>""</TableCellLayout></TableCell>
                                                                <TableCell class="text-right"><TableCellLayout>"—"</TableCellLayout></TableCell>
                                                                <TableCell class="text-right"><TableCellLayout>"—"</TableCellLayout></TableCell>
                                                                <TableCell class="text-right"><TableCellLayout>"—"</TableCellLayout></TableCell>
                                                            </TableRow>
                                                        </Show>
                                                        <For
                                                            each=move || orders_stored.get_value()
                                                            key=|order| order.order_key.clone()
                                                            children=move |order| {
                                                                let price = order.finished_price.map(fmt_money).unwrap_or_else(|| "—".to_string());
                                                                let price_basis = if order.allocation_basis.abs() > f64::EPSILON {
                                                                    fmt_money(order.allocation_basis)
                                                                } else {
                                                                    price
                                                                };
                                                                let allocated = if order.is_allocated {
                                                                    fmt_money(order.allocated_cost)
                                                                } else {
                                                                    "—".to_string()
                                                                };
                                                                let ratio_str = if order.is_allocated {
                                                                    fmt_expense_share(order.allocated_cost, total_expense)
                                                                } else {
                                                                    "—".to_string()
                                                                };
                                                                let (status_color, status_label) = if order.is_cancel {
                                                                    (BadgeColor::Danger, "Отменён")
                                                                } else if !order.is_allocated {
                                                                    (BadgeColor::Warning, "Не в выборке")
                                                                } else {
                                                                    (BadgeColor::Success, "Активен")
                                                                };
                                                                let order_date_display = order.order_date.as_deref().map(fmt_date).unwrap_or_else(|| order.order_key.clone());
                                                                let order_id_val = order.order_id.clone().unwrap_or_default();
                                                                let order_key_display = order.order_key.clone();
                                                                view! {
                                                                    <TableRow>
                                                                        // Наименование → "Заказ dd.mm.yyyy"
                                                                        <TableCell><TableCellLayout truncate=true>
                                                                            <span class="table__tree-child-marker" aria-hidden="true">"└─ "</span>
                                                                            {format!("Заказ {}", order_date_display)}
                                                                        </TableCellLayout></TableCell>
                                                                        // Артикул/Заказ → srid as link to a015
                                                                        <TableCell><TableCellLayout truncate=true>
                                                                            {if order_id_val.is_empty() {
                                                                                view! { <span>{order_key_display}</span> }.into_any()
                                                                            } else {
                                                                                view! {
                                                                                    <a href="#" class="table__link" on:click=move |e| {
                                                                                        e.prevent_default();
                                                                                        open_order.run(order_id_val.clone());
                                                                                    }>{order_key_display}</a>
                                                                                }.into_any()
                                                                            }}
                                                                        </TableCellLayout></TableCell>
                                                                        <TableCell class="text-right"><TableCellLayout>""</TableCellLayout></TableCell>
                                                                        <TableCell class="text-right"><TableCellLayout>""</TableCellLayout></TableCell>
                                                                        <TableCell>
                                                                            <TableCellLayout truncate=true>
                                                                                <Badge appearance=BadgeAppearance::Tint color=status_color>{status_label}</Badge>
                                                                            </TableCellLayout>
                                                                        </TableCell>
                                                                        <TableCell class="text-right"><TableCellLayout>{price_basis}</TableCellLayout></TableCell>
                                                                        <TableCell class="text-right"><TableCellLayout>{allocated}</TableCellLayout></TableCell>
                                                                        <TableCell class="text-right"><TableCellLayout>{ratio_str}</TableCellLayout></TableCell>
                                                                    </TableRow>
                                                                }
                                                            }
                                                        />
                                                        </Show>
                                                    }
                                                }
                                            />
                                        </TableBody>
                                    </Table>
                                </div>
                            }.into_any()
                        }
                    }
                </CardAnimated>
            </div>
        }.into_any()
    }
}
