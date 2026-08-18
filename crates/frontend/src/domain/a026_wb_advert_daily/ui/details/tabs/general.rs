//! General tab - document, links, metrics and technical info.

use super::super::api::{fmt_advert_id, fmt_date, fmt_dt, fmt_money};
use super::super::view_model::WbAdvertDailyDetailsVm;
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
pub fn GeneralTab(vm: WbAdvertDailyDetailsVm) -> impl IntoView {
    let doc = vm.doc;
    view! {
        {move || {
            let Some(d) = doc.get() else {
                return view! { <div class="text-muted">"Нет данных"</div> }.into_any();
            };
            view! {
                <div class="detail-grid">
                    <div class="detail-grid__col">
                        <CardAnimated delay_ms=0 nav_id="a026_wb_advert_daily_details_general_document">
                            <h4 class="details-section__title">"Документ"</h4>
                            <div style="display:grid;grid-template-columns:1fr 1fr;gap:var(--spacing-sm);">
                                <ReadField label="Номер" value=d.document_no.clone() />
                                <ReadField label="Дата" value=fmt_date(&d.document_date) />
                            </div>
                            <div style="display:grid;grid-template-columns:1fr 1fr;gap:var(--spacing-sm);">
                                <ReadField label="Кампания (advert_id)" value=fmt_advert_id(d.advert_id) />
                                <ReadField label="Статус" value=if d.is_posted { "Проведен".to_string() } else { "Не проведен".to_string() } />
                            </div>
                            <ReadField label="ID" value=d.id.clone() />
                        </CardAnimated>

                        <CardAnimated delay_ms=80 nav_id="a026_wb_advert_daily_details_general_links">
                            <h4 class="details-section__title">"Связи"</h4>
                            <ReadField label="Кабинет" value=d.connection_name.clone().unwrap_or(d.connection_id.clone()) />
                            <ReadField label="Организация" value=d.organization_name.clone().unwrap_or(d.organization_id.clone()) />
                            <ReadField label="Маркетплейс" value=d.marketplace_name.clone().unwrap_or(d.marketplace_id.clone()) />
                        </CardAnimated>
                    </div>
                    <div class="detail-grid__col">
                        <CardAnimated delay_ms=40 nav_id="a026_wb_advert_daily_details_general_metrics">
                            <h4 class="details-section__title">"Метрики"</h4>
                            <div style="display:grid;grid-template-columns:1fr 1fr;gap:var(--spacing-sm);">
                                <ReadField label="Итоговый расход" value=fmt_money(d.totals.sum) />
                                <ReadField label="Не распределено" value=fmt_money(d.unattributed_totals.sum) />
                            </div>
                            <div style="display:grid;grid-template-columns:1fr 1fr 1fr;gap:var(--spacing-sm);">
                                <ReadField label="Просмотры" value=d.totals.views.to_string() />
                                <ReadField label="Клики" value=d.totals.clicks.to_string() />
                                <ReadField label="Заказы" value=d.totals.orders.to_string() />
                            </div>
                        </CardAnimated>

                        <CardAnimated delay_ms=120 nav_id="a026_wb_advert_daily_details_general_technical">
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
