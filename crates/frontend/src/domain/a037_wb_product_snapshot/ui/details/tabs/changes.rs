//! Changes tab ("Изменения") - позиции с изменившимся рейтингом/оценкой vs предыдущий снимок.

use super::super::api::{fmt_ratio, RatingChangeDto};
use super::super::view_model::WbProductSnapshotDetailsVm;
use crate::shared::components::card_animated::CardAnimated;
use leptos::prelude::*;
use thaw::*;

/// Форматирует дельту со знаком: +0.05 / -0.10 / 0.00.
fn fmt_delta(v: f64) -> String {
    if v > 0.0 {
        format!("+{:.2}", v)
    } else {
        format!("{:.2}", v)
    }
}

/// Inline-стиль цвета дельты: рост — зелёный, падение — красный.
fn delta_style(v: f64) -> &'static str {
    if v > 0.0 {
        "color: var(--color-success, #2e7d32); font-weight: 600;"
    } else if v < 0.0 {
        "color: var(--color-danger, #c62828); font-weight: 600;"
    } else {
        "color: var(--color-text-muted, #888);"
    }
}

#[component]
pub fn ChangesTab(vm: WbProductSnapshotDetailsVm) -> impl IntoView {
    let changes = vm.changes;
    let loading = vm.changes_loading;
    let has_previous = vm.changes_has_previous;
    let prev_date = vm.changes_prev_date;

    let open_product = Callback::new({
        let vm = vm.clone();
        move |product_ref: String| vm.open_product(product_ref)
    });

    view! {
        <CardAnimated delay_ms=0 nav_id="a037_wb_product_snapshot_details_changes_table">
            {move || {
                if loading.get() {
                    return view! {
                        <Flex gap=FlexGap::Small style="align-items:center;padding:var(--spacing-lg);">
                            <Spinner />
                            <span>"Загрузка изменений..."</span>
                        </Flex>
                    }.into_any();
                }
                if !has_previous.get() {
                    return view! {
                        <div class="alert">"Предыдущий снимок по этому кабинету не найден — сравнивать не с чем."</div>
                    }.into_any();
                }

                let rows = changes.get();
                let subtitle = prev_date
                    .get()
                    .map(|d| format!("Сравнение со снимком от {}", d))
                    .unwrap_or_else(|| "Сравнение с предыдущим снимком".to_string());
                let count = rows.len();

                view! {
                    <div style="display:flex;gap:12px;flex-wrap:wrap;align-items:center;margin-bottom:var(--spacing-sm);">
                        <Badge appearance=BadgeAppearance::Tint color=BadgeColor::Informative>
                            {format!("Изменений: {}", count)}
                        </Badge>
                        <span class="text-muted">{subtitle}</span>
                    </div>

                    {if count == 0 {
                        view! { <div class="alert">"Рейтинг и оценка не изменились ни у одной позиции из пересечения."</div> }.into_any()
                    } else {
                        view! {
                            <div class="table-wrapper">
                                <Table attr:style="width:100%;min-width:900px;">
                                    <TableHeader>
                                        <TableRow>
                                            <TableHeaderCell>"nmID"</TableHeaderCell>
                                            <TableHeaderCell>"Наименование"</TableHeaderCell>
                                            <TableHeaderCell>"Артикул продавца"</TableHeaderCell>
                                            <TableHeaderCell>"Артикул 1С"</TableHeaderCell>
                                            <TableHeaderCell><div style="text-align:right;width:100%;">"Рейтинг (было → стало)"</div></TableHeaderCell>
                                            <TableHeaderCell><div style="text-align:right;width:100%;">"Δ Рейтинг"</div></TableHeaderCell>
                                            <TableHeaderCell><div style="text-align:right;width:100%;">"Оценка (было → стало)"</div></TableHeaderCell>
                                            <TableHeaderCell><div style="text-align:right;width:100%;">"Δ Оценка"</div></TableHeaderCell>
                                        </TableRow>
                                    </TableHeader>
                                    <TableBody>
                                        <For
                                            each=move || changes.get()
                                            key=|row: &RatingChangeDto| row.nm_id
                                            children=move |row| {
                                                let article = row.nomenclature_article.clone().unwrap_or_else(|| "—".to_string());
                                                let product_ref_val = row.marketplace_product_ref.clone().unwrap_or_default();
                                                view! {
                                                    <TableRow>
                                                        <TableCell><TableCellLayout>
                                                            {if product_ref_val.is_empty() {
                                                                view! { <span>{row.nm_id}</span> }.into_any()
                                                            } else {
                                                                view! {
                                                                    <a href="#" class="table__link" on:click=move |e| {
                                                                        e.prevent_default();
                                                                        open_product.run(product_ref_val.clone());
                                                                    }>{row.nm_id}</a>
                                                                }.into_any()
                                                            }}
                                                        </TableCellLayout></TableCell>
                                                        <TableCell><TableCellLayout truncate=true>{row.title}</TableCellLayout></TableCell>
                                                        <TableCell><TableCellLayout truncate=true>{row.vendor_code}</TableCellLayout></TableCell>
                                                        <TableCell><TableCellLayout truncate=true>{article}</TableCellLayout></TableCell>
                                                        <TableCell class="text-right"><TableCellLayout>
                                                            {format!("{} → {}", fmt_ratio(row.product_rating_old), fmt_ratio(row.product_rating_new))}
                                                        </TableCellLayout></TableCell>
                                                        <TableCell class="text-right"><TableCellLayout>
                                                            <span style=delta_style(row.product_rating_delta)>{fmt_delta(row.product_rating_delta)}</span>
                                                        </TableCellLayout></TableCell>
                                                        <TableCell class="text-right"><TableCellLayout>
                                                            {format!("{} → {}", fmt_ratio(row.feedback_rating_old), fmt_ratio(row.feedback_rating_new))}
                                                        </TableCellLayout></TableCell>
                                                        <TableCell class="text-right"><TableCellLayout>
                                                            <span style=delta_style(row.feedback_rating_delta)>{fmt_delta(row.feedback_rating_delta)}</span>
                                                        </TableCellLayout></TableCell>
                                                    </TableRow>
                                                }
                                            }
                                        />
                                    </TableBody>
                                </Table>
                            </div>
                        }.into_any()
                    }}
                }.into_any()
            }}
        </CardAnimated>
    }
}
