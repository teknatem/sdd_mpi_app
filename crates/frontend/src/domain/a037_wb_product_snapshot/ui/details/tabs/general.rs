//! General tab - document, links, totals and technical info.

use super::super::api::{fmt_date, fmt_dt, fmt_money};
use super::super::view_model::WbProductSnapshotDetailsVm;
use crate::shared::components::card_animated::CardAnimated;
use leptos::prelude::*;
use thaw::*;

#[component]
fn ReadField(label: &'static str, value: String) -> impl IntoView {
    view! {
        <div class="form__group">
            <label class="form__label">{label}</label>
            <Input value=RwSignal::new(value) attr:readonly=true />
        </div>
    }
}

#[component]
pub fn GeneralTab(vm: WbProductSnapshotDetailsVm) -> impl IntoView {
    let doc = vm.doc;
    view! {
        {move || {
            let Some(d) = doc.get() else {
                return view! { <div class="text-muted">"Нет данных"</div> }.into_any();
            };
            view! {
                <div class="detail-grid">
                    <div class="detail-grid__col">
                        <CardAnimated delay_ms=0 nav_id="a037_wb_product_snapshot_details_general_document">
                            <h4 class="details-section__title">"Документ"</h4>
                            <div style="display:grid;grid-template-columns:1fr 1fr;gap:var(--spacing-sm);">
                                <ReadField label="Номер" value=d.document_no.clone() />
                                <ReadField label="Дата" value=fmt_date(&d.document_date) />
                            </div>
                            <ReadField label="Позиций" value=d.lines.len().to_string() />
                            <ReadField label="ID" value=d.id.clone() />
                        </CardAnimated>

                        <CardAnimated delay_ms=80 nav_id="a037_wb_product_snapshot_details_general_links">
                            <h4 class="details-section__title">"Связи"</h4>
                            <ReadField label="Кабинет" value=d.connection_name.clone().unwrap_or(d.connection_id.clone()) />
                            <ReadField label="Организация" value=d.organization_name.clone().unwrap_or(d.organization_id.clone()) />
                            <ReadField label="Маркетплейс" value=d.marketplace_name.clone().unwrap_or(d.marketplace_id.clone()) />
                        </CardAnimated>
                    </div>
                    <div class="detail-grid__col">
                        <CardAnimated delay_ms=40 nav_id="a037_wb_product_snapshot_details_general_totals">
                            <h4 class="details-section__title">"Итоги остатков"</h4>
                            <div style="display:grid;grid-template-columns:1fr 1fr;gap:var(--spacing-sm);">
                                <ReadField label="Остаток WB, шт" value=d.total_stock_wb.to_string() />
                                <ReadField label="Остаток продавца, шт" value=d.total_stock_mp.to_string() />
                            </div>
                            <ReadField label="Сумма остатков" value=fmt_money(d.total_balance_sum) />
                        </CardAnimated>

                        <CardAnimated delay_ms=120 nav_id="a037_wb_product_snapshot_details_general_technical">
                            <h4 class="details-section__title">"Технические данные"</h4>
                            <ReadField label="Источник" value=d.source.clone() />
                            <ReadField label="Загружено" value=fmt_dt(&d.fetched_at) />
                            <div style="display:grid;grid-template-columns:1fr 1fr;gap:var(--spacing-sm);">
                                <ReadField label="Создано" value=fmt_dt(&d.created_at) />
                                <ReadField label="Обновлено" value=fmt_dt(&d.updated_at) />
                            </div>
                        </CardAnimated>
                    </div>
                </div>
            }.into_any()
        }}
    }
}
