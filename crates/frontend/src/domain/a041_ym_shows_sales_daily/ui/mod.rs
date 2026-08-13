use crate::{
    layout::global_context::AppGlobalContext,
    shared::{
        api_utils::api_base,
        date_utils::format_date,
        list_utils::{get_sort_class, get_sort_indicator},
        page_frame::PageFrame,
    },
};
use chrono::{Duration, Utc};
use contracts::domain::a041_ym_shows_sales_daily::aggregate::{
    YmShowsSalesDailyLine, YmShowsSalesDailyMetrics,
};
use gloo_net::http::Request;
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde::Deserialize;
use std::cmp::Ordering;
use thaw::*;

#[derive(Debug, Clone, Deserialize)]
struct ListItem {
    id: String,
    document_no: String,
    document_date: String,
    lines_count: i32,
    total_shows: Option<i64>,
    total_clicks: Option<i64>,
    total_to_cart: Option<i64>,
    total_order_items: Option<i64>,
    connection_id: String,
    connection_name: Option<String>,
    organization_name: Option<String>,
    fetched_at: String,
}

#[derive(Debug, Deserialize)]
struct ListResponse {
    items: Vec<ListItem>,
    total: usize,
}

fn metric(value: Option<i64>) -> String {
    value.map(|v| v.to_string()).unwrap_or_else(|| "—".into())
}

#[component]
pub fn YmShowsSalesDailyList() -> impl IntoView {
    let tabs = use_context::<AppGlobalContext>().expect("AppGlobalContext not found");
    let today = Utc::now().date_naive();
    let (date_from, set_date_from) =
        signal((today - Duration::days(29)).format("%Y-%m-%d").to_string());
    let (date_to, set_date_to) = signal(today.format("%Y-%m-%d").to_string());
    let (items, set_items) = signal(Vec::<ListItem>::new());
    let (total, set_total) = signal(0usize);
    let (loading, set_loading) = signal(false);
    let (error, set_error) = signal(None::<String>);

    let load = move || {
        spawn_local(async move {
            set_loading.set(true);
            set_error.set(None);
            let url = format!(
                "{}/api/a041/ym-shows-sales/list?date_from={}&date_to={}&limit=500",
                api_base(),
                date_from.get(),
                date_to.get()
            );
            match Request::get(&url).send().await {
                Ok(resp) if resp.ok() => match resp.json::<ListResponse>().await {
                    Ok(data) => {
                        set_total.set(data.total);
                        set_items.set(data.items)
                    }
                    Err(e) => set_error.set(Some(format!("Ошибка разбора: {e}"))),
                },
                Ok(resp) => set_error.set(Some(format!("Ошибка сервера: HTTP {}", resp.status()))),
                Err(e) => set_error.set(Some(format!("Ошибка сети: {e}"))),
            }
            set_loading.set(false);
        })
    };
    Effect::new(move |_| load());

    view! { <PageFrame page_id="a041_ym_shows_sales_daily--list" category="list" class="page--wide">
        <div class="page__header"><div class="page__header-left"><h1 class="page__title">"Воронка продаж Yandex Market"</h1></div></div>
        <div class="page__content">
            <Flex gap=FlexGap::Medium align=FlexAlign::Center>
                <label>"Период:"</label>
                <input class="doc-filter__input" type="date" prop:value=move||date_from.get() on:change=move|e|set_date_from.set(event_target_value(&e))/>
                <span>"—"</span>
                <input class="doc-filter__input" type="date" prop:value=move||date_to.get() on:change=move|e|set_date_to.set(event_target_value(&e))/>
                <Button appearance=ButtonAppearance::Primary on_click=move|_|load()>"Обновить"</Button>
                <span>{move||format!("Документов: {}",total.get())}</span>
            </Flex>
            {move||error.get().map(|e|view!{<div class="alert alert--error" style="margin-top:12px;">{e}</div>})}
            <div class="table-wrapper" style="margin-top:16px;">
            {move||if loading.get(){view!{<Flex justify=FlexJustify::Center><Spinner/></Flex>}.into_any()}else{view!{
                <Table attr:style="width:100%;min-width:1000px;"><TableHeader><TableRow>
                    <TableHeaderCell>"Дата"</TableHeaderCell><TableHeaderCell>"Документ"</TableHeaderCell>
                    <TableHeaderCell>"Кабинет"</TableHeaderCell><TableHeaderCell>"Организация"</TableHeaderCell>
                    <TableHeaderCell>"Товаров"</TableHeaderCell>
                    <TableHeaderCell>"Показы"</TableHeaderCell><TableHeaderCell>"Клики"</TableHeaderCell>
                    <TableHeaderCell>"В корзину"</TableHeaderCell><TableHeaderCell>"Заказано"</TableHeaderCell><TableHeaderCell>"Загружено"</TableHeaderCell>
                </TableRow></TableHeader><TableBody><For each=move||items.get() key=|x|x.id.clone() children=move|x|{
                    let id=x.id.clone();let date=x.document_date.clone();let title=format!("Воронка YM {}",date);
                    let connection=x.connection_name.clone().unwrap_or_else(||x.connection_id.clone());
                    view!{<TableRow><TableCell>{x.document_date}</TableCell><TableCell><a href="#" class="table__link" on:click=move|e|{e.prevent_default();tabs.open_tab(&format!("a041_ym_shows_sales_daily_details_{id}"),&title)}>{x.document_no}</a></TableCell>
                    <TableCell>{connection}</TableCell><TableCell>{x.organization_name.unwrap_or_else(||"—".into())}</TableCell>
                    <TableCell>{x.lines_count}</TableCell>
                    <TableCell>{metric(x.total_shows)}</TableCell><TableCell>{metric(x.total_clicks)}</TableCell>
                    <TableCell>{metric(x.total_to_cart)}</TableCell><TableCell>{metric(x.total_order_items)}</TableCell><TableCell>{x.fetched_at}</TableCell></TableRow>}
                }/></TableBody></Table>
            }.into_any()}}
            </div>
        </div>
    </PageFrame> }
}

#[derive(Debug, Clone, Deserialize)]
struct DetailsDto {
    document_no: String,
    document_date: String,
    connection_id: String,
    totals: YmShowsSalesDailyMetrics,
    fetched_at: String,
    lines: Vec<YmShowsSalesDailyLine>,
}

fn compare_optional_i64(left: Option<i64>, right: Option<i64>) -> Ordering {
    match (left, right) {
        (Some(left), Some(right)) => left.cmp(&right),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

fn compare_lines(
    left: &YmShowsSalesDailyLine,
    right: &YmShowsSalesDailyLine,
    field: &str,
) -> Ordering {
    match field {
        "offer_name" => left
            .offer_name
            .to_lowercase()
            .cmp(&right.offer_name.to_lowercase()),
        "shows" => compare_optional_i64(left.metrics.shows, right.metrics.shows),
        "clicks" => compare_optional_i64(left.metrics.clicks, right.metrics.clicks),
        "to_cart" => compare_optional_i64(left.metrics.to_cart, right.metrics.to_cart),
        "order_items" => compare_optional_i64(left.metrics.order_items, right.metrics.order_items),
        "delivered_count" => {
            compare_optional_i64(left.metrics.delivered_count, right.metrics.delivered_count)
        }
        "canceled_count" => {
            compare_optional_i64(left.metrics.canceled_count, right.metrics.canceled_count)
        }
        "returned_count" => {
            compare_optional_i64(left.metrics.returned_count, right.metrics.returned_count)
        }
        _ => left
            .offer_id
            .to_lowercase()
            .cmp(&right.offer_id.to_lowercase()),
    }
}

#[component]
pub fn YmShowsSalesDailyDetail(id: String, #[prop(into)] on_close: Callback<()>) -> impl IntoView {
    let (doc, set_doc) = signal(None::<DetailsDto>);
    let (error, set_error) = signal(None::<String>);
    let (sort_field, set_sort_field) = signal("offer_id".to_string());
    let (sort_ascending, set_sort_ascending) = signal(true);
    let toggle_sort = move |field: &'static str| {
        if sort_field.get_untracked() == field {
            set_sort_ascending.update(|value| *value = !*value);
        } else {
            set_sort_field.set(field.to_string());
            set_sort_ascending.set(true);
        }
    };
    spawn_local(async move {
        let url = format!("{}/api/a041/ym-shows-sales/{}", api_base(), id);
        match Request::get(&url).send().await {
            Ok(resp) if resp.ok() => match resp.json().await {
                Ok(value) => set_doc.set(Some(value)),
                Err(e) => set_error.set(Some(e.to_string())),
            },
            Ok(resp) => set_error.set(Some(format!("HTTP {}", resp.status()))),
            Err(e) => set_error.set(Some(e.to_string())),
        }
    });
    view! {<PageFrame page_id="a041_ym_shows_sales_daily--detail" category="detail" class="page--wide">
        <div class="page__header"><div class="page__header-left"><h1 class="page__title">{move || doc.get().map(|d| format!("Воронка продаж YM от {}", format_date(&d.document_date))).unwrap_or_else(|| "Воронка продаж YM".to_string())}</h1></div>
        <div class="page__header-right"><Button on_click=move|_|on_close.run(())>"Закрыть"</Button></div></div>
        <div class="page__content">{move||if let Some(e)=error.get(){view!{<div class="alert alert--error">{e}</div>}.into_any()}else if let Some(d)=doc.get(){let lines=d.lines.clone();view!{
            <Card><Flex gap=FlexGap::Large><span><b>"Дата: "</b>{format_date(&d.document_date)}</span><span><b>"Документ: "</b>{d.document_no}</span><span><b>"Кабинет: "</b>{d.connection_id}</span><span><b>"Показы: "</b>{metric(d.totals.shows)}</span><span><b>"Клики: "</b>{metric(d.totals.clicks)}</span><span><b>"Загружено: "</b>{d.fetched_at}</span></Flex></Card>
            <div class="table-wrapper" style="margin-top:16px;"><Table attr:style="width:100%;min-width:950px;"><TableHeader><TableRow>
                {[
                    ("offer_id", "Offer ID", 130.0),
                    ("offer_name", "Товар", 260.0),
                    ("shows", "Показы", 90.0),
                    ("clicks", "Клики", 80.0),
                    ("to_cart", "В корзину", 100.0),
                    ("order_items", "Заказано", 95.0),
                    ("delivered_count", "Доставлено", 105.0),
                    ("canceled_count", "Отменено", 95.0),
                    ("returned_count", "Возвращено", 105.0),
                ].into_iter().map(|(field, label, min_width)| view!{
                    <TableHeaderCell resizable=false min_width=min_width class="resizable">
                        <div class="table__sortable-header" style="cursor:pointer;" on:click=move |_| toggle_sort(field)>
                            {label}
                            <span class=move || get_sort_class(&sort_field.get(), field)>
                                {move || get_sort_indicator(&sort_field.get(), field, sort_ascending.get())}
                            </span>
                        </div>
                    </TableHeaderCell>
                }).collect_view()}
            </TableRow></TableHeader>
            <TableBody><For each=move||{let mut sorted=lines.clone();let field=sort_field.get();let ascending=sort_ascending.get();sorted.sort_by(|left,right|{let ordering=compare_lines(left,right,&field);if ascending{ordering}else{ordering.reverse()}});sorted} key=|x|x.offer_id.clone() children=move|x|view!{<TableRow><TableCell>{x.offer_id}</TableCell><TableCell>{x.offer_name}</TableCell><TableCell>{metric(x.metrics.shows)}</TableCell><TableCell>{metric(x.metrics.clicks)}</TableCell><TableCell>{metric(x.metrics.to_cart)}</TableCell><TableCell>{metric(x.metrics.order_items)}</TableCell><TableCell>{metric(x.metrics.delivered_count)}</TableCell><TableCell>{metric(x.metrics.canceled_count)}</TableCell><TableCell>{metric(x.metrics.returned_count)}</TableCell></TableRow>}/></TableBody></Table></div>
        }.into_any()}else{view!{<Flex justify=FlexJustify::Center><Spinner/></Flex>}.into_any()}}</div>
    </PageFrame>}
}
