use crate::{
    layout::global_context::AppGlobalContext,
    shared::{api_utils::api_base, page_frame::PageFrame},
};
use gloo_net::http::Request;
use leptos::{prelude::*, task::spawn_local};
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
struct Row {
    id: String,
    report_id: String,
    date_from: String,
    date_to: String,
    create_date: String,
    seller_finance_name: String,
    currency: String,
    for_pay_sum: Option<String>,
    bank_payment_sum: Option<String>,
    lines_count: i32,
    connection_name: Option<String>,
}
#[derive(Clone, Debug, Deserialize)]
struct Page {
    items: Vec<Row>,
    total: usize,
}

#[component]
pub fn WbFinanceReportsList() -> impl IntoView {
    let tabs = leptos::context::use_context::<AppGlobalContext>().expect("AppGlobalContext");
    let (rows, set_rows) = signal(Vec::<Row>::new());
    let (total, set_total) = signal(0usize);
    let (loading, set_loading) = signal(true);
    let (error, set_error) = signal::<Option<String>>(None);
    spawn_local(async move {
        let url = format!("{}/api/a043/wb-finance-reports/list?period=daily&limit=100&sort_by=create_date&sort_desc=true", api_base());
        match Request::get(&url).send().await {
            Ok(response) if response.ok() => match response.json::<Page>().await {
                Ok(page) => {
                    set_total.set(page.total);
                    set_rows.set(page.items);
                }
                Err(e) => set_error.set(Some(format!("Ошибка чтения ответа: {e}"))),
            },
            Ok(response) => set_error.set(Some(format!("Ошибка сервера: {}", response.status()))),
            Err(e) => set_error.set(Some(format!("Ошибка сети: {e}"))),
        }
        set_loading.set(false);
    });

    view! {
        <PageFrame page_id="a043_wb_finance_report--list" category="list" class="page--wide">
            <div class="page__header"><div class="page__header-left">
                <h1 class="page__title">"Финансовые отчёты WB (новый API)"</h1>
                <span class="page__subtitle">{move || format!("Документов: {}", total.get())}</span>
            </div></div>
            <div class="page__content">
                {move || if loading.get() { view!{<div>"Загрузка..."</div>}.into_any() }
                    else if let Some(e)=error.get() { view!{<div class="alert alert--error">{e}</div>}.into_any() }
                    else { view!{
                        <div class="table-wrapper"><table class="table" style="width:100%;min-width:1100px">
                            <thead><tr><th>"Report ID"</th><th>"Период"</th><th>"Создан"</th><th>"Продавец"</th><th>"К перечислению"</th><th>"Банк"</th><th>"Строк"</th><th>"Кабинет"</th></tr></thead>
                            <tbody><For each=move || rows.get() key=|r| r.id.clone() children=move |r| {
                                let id=r.id.clone(); let report=r.report_id.clone(); let tabs=tabs.clone();
                                view!{<tr class="table__row--clickable" on:click=move |_| tabs.open_tab(&format!("a043_wb_finance_report_details_{id}"), &format!("WB Finance {report}"))>
                                    <td>{r.report_id}</td><td>{format!("{} – {}",r.date_from,r.date_to)}</td><td>{r.create_date}</td><td>{r.seller_finance_name}</td>
                                    <td>{format!("{} {}",r.for_pay_sum.unwrap_or_default(),r.currency)}</td><td>{r.bank_payment_sum.unwrap_or_default()}</td><td>{r.lines_count}</td><td>{r.connection_name.unwrap_or_default()}</td>
                                </tr>}
                            }/></tbody>
                        </table></div>
                    }.into_any() }}
            </div>
        </PageFrame>
    }
}
