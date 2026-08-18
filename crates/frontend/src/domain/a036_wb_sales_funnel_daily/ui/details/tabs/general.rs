//! General tab - document, links, totals and technical info.

use super::super::api::{fmt_date, fmt_dt, fmt_money, fmt_ratio};
use super::super::view_model::WbSalesFunnelDailyDetailsVm;
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
pub fn GeneralTab(vm: WbSalesFunnelDailyDetailsVm) -> impl IntoView {
    let doc = vm.doc;
    view! {
        {move || {
            let Some(d) = doc.get() else {
                return view! { <div class="text-muted">"Нет данных"</div> }.into_any();
            };
            view! {
                <div class="detail-grid">
                    <div class="detail-grid__col">
                        <CardAnimated delay_ms=0 nav_id="a036_wb_sales_funnel_daily_details_general_document">
                            <h4 class="details-section__title">"Документ"</h4>
                            <div style="display:grid;grid-template-columns:1fr 1fr;gap:var(--spacing-sm);">
                                <ReadField label="Номер" value=d.document_no.clone() />
                                <ReadField label="Дата" value=fmt_date(&d.document_date) />
                            </div>
                            <div style="display:grid;grid-template-columns:1fr 1fr;gap:var(--spacing-sm);">
                                <ReadField label="Валюта" value=if d.currency.is_empty() { "—".to_string() } else { d.currency.clone() } />
                                <ReadField label="Позиций" value=d.lines.len().to_string() />
                            </div>
                            <ReadField label="ID" value=d.id.clone() />
                        </CardAnimated>

                        <CardAnimated delay_ms=80 nav_id="a036_wb_sales_funnel_daily_details_general_links">
                            <h4 class="details-section__title">"Связи"</h4>
                            <ReadField label="Кабинет" value=d.connection_name.clone().unwrap_or(d.connection_id.clone()) />
                            <ReadField label="Организация" value=d.organization_name.clone().unwrap_or(d.organization_id.clone()) />
                            <ReadField label="Маркетплейс" value=d.marketplace_name.clone().unwrap_or(d.marketplace_id.clone()) />
                        </CardAnimated>
                    </div>
                    <div class="detail-grid__col">
                        <CardAnimated delay_ms=40 nav_id="a036_wb_sales_funnel_daily_details_general_metrics">
                            <h4 class="details-section__title">"Итоги воронки"</h4>
                            <div style="display:grid;grid-template-columns:1fr 1fr 1fr;gap:var(--spacing-sm);">
                                <ReadField label="Переходы" value=d.totals.open_count.to_string() />
                                <ReadField label="В корзину" value=d.totals.cart_count.to_string() />
                                <ReadField label="Отложенные" value=d.totals.add_to_wishlist_count.to_string() />
                            </div>
                            <div style="display:grid;grid-template-columns:1fr 1fr;gap:var(--spacing-sm);">
                                <ReadField label="Заказы, шт" value=d.totals.order_count.to_string() />
                                <ReadField label="Сумма заказов" value=fmt_money(d.totals.order_sum) />
                            </div>
                            <div style="display:grid;grid-template-columns:1fr 1fr;gap:var(--spacing-sm);">
                                <ReadField label="Выкупы, шт" value=d.totals.buyout_count.to_string() />
                                <ReadField label="Сумма выкупов" value=fmt_money(d.totals.buyout_sum) />
                            </div>
                            <div style="display:grid;grid-template-columns:1fr 1fr;gap:var(--spacing-sm);">
                                <ReadField label="Отмены, шт" value=d.totals.cancel_count.map(|v| v.to_string()).unwrap_or_else(|| "—".to_string()) />
                                <ReadField label="Сумма отмен" value=d.totals.cancel_sum.map(fmt_money).unwrap_or_else(|| "—".to_string()) />
                            </div>
                            <div style="display:grid;grid-template-columns:1fr 1fr 1fr;gap:var(--spacing-sm);">
                                <ReadField label="Конв. в корзину, %" value=fmt_ratio(d.totals.add_to_cart_conversion) />
                                <ReadField label="Конв. в заказ, %" value=fmt_ratio(d.totals.cart_to_order_conversion) />
                                <ReadField label="Процент выкупа, %" value=fmt_ratio(d.totals.buyout_percent) />
                            </div>
                        </CardAnimated>

                        <CardAnimated delay_ms=120 nav_id="a036_wb_sales_funnel_daily_details_general_technical">
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
