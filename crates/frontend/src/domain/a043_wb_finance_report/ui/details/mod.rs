use crate::shared::{api_utils::api_base, page_frame::PageFrame, page_standard::PAGE_CAT_DETAIL};
use contracts::domain::a043_wb_finance_report::{WbFinanceReportHeader, WbFinanceReportSourceMeta};
use gloo_net::http::Request;
use leptos::{prelude::*, task::spawn_local};
use serde::Deserialize;
use serde_json::Value;
use thaw::{Button, ButtonAppearance};

#[derive(Clone, Debug, Deserialize)]
struct Detail {
    header: WbFinanceReportHeader,
    source_meta: WbFinanceReportSourceMeta,
    lines_count: usize,
}
#[derive(Clone, Debug, Deserialize)]
struct Lines {
    items: Vec<Value>,
    total: usize,
}

#[component]
pub fn WbFinanceReportDetail(id: String, #[prop(into)] on_close: Callback<()>) -> impl IntoView {
    let (detail, set_detail) = signal::<Option<Detail>>(None);
    let (lines, set_lines) = signal(Vec::<Value>::new());
    let (lines_total, set_lines_total) = signal(0usize);
    let (lines_offset, set_lines_offset) = signal(0usize);
    let (error, set_error) = signal::<Option<String>>(None);
    let detail_id = id.clone();
    spawn_local(async move {
        let base = api_base();
        match Request::get(&format!("{base}/api/a043/wb-finance-reports/{detail_id}"))
            .send()
            .await
        {
            Ok(r) if r.ok() => match r.json::<Detail>().await {
                Ok(v) => set_detail.set(Some(v)),
                Err(e) => set_error.set(Some(e.to_string())),
            },
            Ok(r) => set_error.set(Some(format!("Ошибка сервера: {}", r.status()))),
            Err(e) => set_error.set(Some(e.to_string())),
        }
    });
    let lines_id = id.clone();
    Effect::new(move |_| {
        let offset = lines_offset.get();
        let lines_id = lines_id.clone();
        spawn_local(async move {
            let url = format!(
                "{}/api/a043/wb-finance-reports/{lines_id}/lines?offset={offset}&limit=100",
                api_base()
            );
            if let Ok(response) = Request::get(&url).send().await {
                if response.ok() {
                    if let Ok(page) = response.json::<Lines>().await {
                        set_lines_total.set(page.total);
                        set_lines.set(page.items);
                    }
                }
            }
        });
    });
    view! {
        <PageFrame page_id="a043_wb_finance_report--detail" category=PAGE_CAT_DETAIL class="page--wide">
            <div class="page__header"><div class="page__header-left"><h1 class="page__title">{move || detail.get().map(|d|format!("WB Finance {}",d.header.report_id)).unwrap_or_else(||"Финансовый отчёт WB".into())}</h1></div>
            <div class="page__header-right"><Button appearance=ButtonAppearance::Secondary on_click=move |_|on_close.run(())>"Закрыть"</Button></div></div>
            <div class="page__content">
                {move || error.get().map(|e|view!{<div class="alert alert--error">{e}</div>})}
                {move || detail.get().map(|d| { let h=d.header; view!{
                    <div class="card" style="padding:16px;margin-bottom:16px">
                        <div><b>"Период: "</b>{format!("{} – {}",h.date_from,h.date_to)}</div>
                        <div><b>"Создан: "</b>{h.create_date}</div>
                        <div><b>"Продавец: "</b>{h.seller_finance_name}</div>
                        <div style="display:grid;grid-template-columns:repeat(auto-fit,minmax(260px,1fr));gap:6px;margin-top:12px">
                            <div><b>"Розничная сумма: "</b>{format!("{} {}",h.retail_amount_sum.unwrap_or_default(),h.currency)}</div>
                            <div><b>"К перечислению: "</b>{format!("{} {}",h.for_pay_sum.unwrap_or_default(),h.currency)}</div>
                            <div><b>"Логистика: "</b>{h.delivery_service_sum.unwrap_or_default()}</div>
                            <div><b>"Хранение: "</b>{h.paid_storage_sum.unwrap_or_default()}</div>
                            <div><b>"Приёмка: "</b>{h.paid_acceptance_sum.unwrap_or_default()}</div>
                            <div><b>"Удержания: "</b>{h.deduction_sum.unwrap_or_default()}</div>
                            <div><b>"Штрафы: "</b>{h.penalty_sum.unwrap_or_default()}</div>
                            <div><b>"Доплаты: "</b>{h.additional_payment_sum.unwrap_or_default()}</div>
                            <div><b>"Cashback: "</b>{h.cashback_amount_sum.unwrap_or_default()}</div>
                            <div><b>"Cashback discount: "</b>{h.cashback_discount_sum.unwrap_or_default()}</div>
                            <div><b>"Cashback commission: "</b>{h.cashback_commission_change_sum.unwrap_or_default()}</div>
                            <div><b>"Банковский платёж: "</b>{h.bank_payment_sum.unwrap_or_default()}</div>
                        </div>
                        <div style="margin-top:12px"><b>"Строк: "</b>{d.lines_count}</div><div><b>"Загружен: "</b>{d.source_meta.fetched_at}</div>
                    </div>
                }})}
                <div style="display:flex;align-items:center;gap:12px;margin-bottom:8px">
                    <h2 style="margin:0">"Строки отчёта"</h2>
                    <span>{move || format!("{}–{} из {}", if lines_total.get()==0 {0} else {lines_offset.get()+1}, (lines_offset.get()+100).min(lines_total.get()), lines_total.get())}</span>
                    <button disabled=move || lines_offset.get()==0 on:click=move |_| set_lines_offset.update(|v| *v=v.saturating_sub(100))>"Назад"</button>
                    <button disabled=move || lines_offset.get()+100>=lines_total.get() on:click=move |_| set_lines_offset.update(|v| *v+=100)>"Вперёд"</button>
                </div>
                <div class="table-wrapper"><table class="table" style="width:100%"><thead><tr><th>"rrdId"</th><th>"nmId"</th><th>"Операция"</th><th>"Сумма"</th><th>"Исходный JSON"</th></tr></thead>
                <tbody><For each=move || lines.get() key=|v| v.get("rrdId").map(Value::to_string).unwrap_or_else(||v.to_string()) children=|v| view! {
                    <tr>
                        <td>{v.get("rrdId").map(Value::to_string).unwrap_or_default()}</td>
                        <td>{v.get("nmId").map(Value::to_string).unwrap_or_default()}</td>
                        <td>{v.get("supplierOperName").and_then(Value::as_str).unwrap_or_default().to_string()}</td>
                        <td>{v.get("retailAmount").map(Value::to_string).unwrap_or_default()}</td>
                        <td><details><summary>"JSON"</summary><pre style="white-space:pre-wrap">{serde_json::to_string_pretty(&v).unwrap_or_default()}</pre></details></td>
                    </tr>
                }/></tbody></table></div>
            </div>
        </PageFrame>
    }
}
