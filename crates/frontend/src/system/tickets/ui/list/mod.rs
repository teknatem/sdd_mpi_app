mod state;

use contracts::system::tickets::{TicketPriority, TicketStatus, TicketType};
use gloo_timers::future::TimeoutFuture;
use leptos::prelude::*;
use leptos::task::spawn_local;
use thaw::*;

use crate::layout::global_context::AppGlobalContext;
use crate::shared::components::pagination_controls::PaginationControls;
use crate::shared::components::table::TableCrosshairHighlight;
use crate::shared::date_utils::format_datetime;
use crate::shared::icons::icon;
use crate::shared::page_frame::PageFrame;
use crate::shared::page_standard::PAGE_CAT_SYSTEM;
use crate::shared::table_utils::init_column_resize;
use crate::system::auth::guard::RequireAuth;
use crate::system::tickets::api::{self, TicketListQuery};
use state::{create_state, TicketsListState};

const TABLE_ID: &str = "sys-tickets-table";
const COLUMN_WIDTHS_KEY: &str = "sys_tickets_column_widths";

pub fn ticket_type_badge_class(ticket_type: TicketType) -> &'static str {
    match ticket_type {
        TicketType::Question => "badge badge--info",
        TicketType::Improvement => "badge badge--primary",
        TicketType::Bug => "badge badge--error",
        TicketType::Idea => "badge badge--success",
    }
}

pub fn ticket_status_badge_class(status: TicketStatus) -> &'static str {
    match status {
        TicketStatus::New => "badge badge--info",
        TicketStatus::InProgress => "badge badge--primary",
        TicketStatus::Waiting => "badge badge--warning",
        TicketStatus::Resolved => "badge badge--success",
        TicketStatus::Closed => "badge badge--neutral",
        TicketStatus::Rejected => "badge badge--error",
    }
}

pub fn ticket_priority_badge_class(priority: TicketPriority) -> &'static str {
    match priority {
        TicketPriority::Low => "badge badge--neutral",
        TicketPriority::Normal => "badge badge--info",
        TicketPriority::High => "badge badge--error",
    }
}

#[component]
pub fn TicketsListPage() -> impl IntoView {
    view! {
        <RequireAuth>
            <TicketsList />
        </RequireAuth>
    }
}

fn recalc_pagination(state: &mut TicketsListState) {
    let total_pages = if state.total_count == 0 {
        1
    } else {
        (state.total_count + state.page_size - 1) / state.page_size
    };
    state.total_pages = total_pages;
    if state.page >= total_pages {
        state.page = total_pages.saturating_sub(1);
    }
}

#[component]
fn TicketsList() -> impl IntoView {
    let state = create_state();
    let (error, set_error) = signal::<Option<String>>(None);
    let (loading, set_loading) = signal(false);

    let global_ctx = use_context::<AppGlobalContext>();

    let open_ticket_details = move |ticket_id: String, code: String| {
        let tab_key = format!("sys_ticket_details_{}", ticket_id);
        if let Some(ctx) = global_ctx {
            ctx.open_tab(&tab_key, &format!("Тикет {}", code));
        }
    };

    let open_new_ticket = move || {
        if let Some(ctx) = global_ctx {
            ctx.open_tab("sys_ticket_new", "Новый тикет");
        }
    };

    let load_data = move || {
        set_loading.set(true);
        set_error.set(None);
        let query = state.with_untracked(|s| TicketListQuery {
            status: Some(s.status_filter.clone()).filter(|v| !v.is_empty()),
            ticket_type: Some(s.type_filter.clone()).filter(|v| !v.is_empty()),
            search: Some(s.search_query.clone()).filter(|v| !v.trim().is_empty()),
            limit: s.page_size as u64,
            offset: (s.page * s.page_size) as u64,
        });
        spawn_local(async move {
            match api::fetch_tickets(&query).await {
                Ok(data) => {
                    state.update(|s| {
                        s.items = data.items;
                        s.total_count = data.total as usize;
                        recalc_pagination(s);
                        s.is_loaded = true;
                    });
                    set_loading.set(false);
                }
                Err(e) => {
                    set_error.set(Some(format!("Не удалось загрузить тикеты: {}", e)));
                    set_loading.set(false);
                }
            }
        });
    };

    Effect::new(move |_| {
        if !state.with_untracked(|s| s.is_loaded) {
            load_data();
        }
    });

    Effect::new(move |_| {
        if state.with(|s| s.is_loaded) {
            spawn_local(async move {
                TimeoutFuture::new(50).await;
                init_column_resize(TABLE_ID, COLUMN_WIDTHS_KEY);
            });
        }
    });

    let search_signal = RwSignal::new(String::new());
    let status_signal = RwSignal::new(String::new());
    let type_signal = RwSignal::new(String::new());

    let apply_filters = move || {
        state.update(|s| {
            s.search_query = search_signal.get_untracked();
            s.status_filter = status_signal.get_untracked();
            s.type_filter = type_signal.get_untracked();
            s.page = 0;
        });
        load_data();
    };

    let reset_filters = move || {
        search_signal.set(String::new());
        status_signal.set(String::new());
        type_signal.set(String::new());
        state.update(|s| {
            s.search_query = String::new();
            s.status_filter = String::new();
            s.type_filter = String::new();
            s.page = 0;
        });
        load_data();
    };

    let go_to_page = move |page: usize| {
        state.update(|s| {
            s.page = page;
        });
        load_data();
    };

    let change_page_size = move |size: usize| {
        state.update(|s| {
            s.page_size = size;
            s.page = 0;
        });
        load_data();
    };

    let format_ts = |value: &str| format_datetime(value);
    // Дедлайн хранится как дата "YYYY-MM-DD" — показываем как есть
    let format_deadline = |value: &Option<String>| value.clone().unwrap_or_else(|| "-".to_string());

    view! {
        <PageFrame page_id="sys_tickets--list" category=PAGE_CAT_SYSTEM class="sys-tickets">
            <div class="page__header">
                <div class="page__header-left">
                    <h1 class="page__title">"Тикеты"</h1>
                    <Badge>
                        {move || state.get().total_count.to_string()}
                    </Badge>
                </div>
                <div class="page__header-right">
                    <Button
                        appearance=ButtonAppearance::Primary
                        on_click=move |_| open_new_ticket()
                    >
                        {icon("plus")}
                        " Новый тикет"
                    </Button>
                    <Button
                        appearance=ButtonAppearance::Secondary
                        on_click=move |_| load_data()
                        disabled=Signal::derive(move || loading.get())
                    >
                        {icon("refresh")}
                        {move || if loading.get() { " Загрузка..." } else { " Обновить" }}
                    </Button>
                </div>
            </div>

            <div class="page__content">
                {move || error.get().map(|e| view! { <div class="alert alert--error">{e}</div> })}

                <div class="filter-panel">
                    <div class="filter-panel-header">
                        <div class="filter-panel-header__left">
                            {icon("filter")}
                            <span class="filter-panel__title">"Фильтры"</span>
                        </div>
                        <div class="filter-panel-header__center">
                            <PaginationControls
                                current_page=Signal::derive(move || state.get().page)
                                total_pages=Signal::derive(move || state.get().total_pages)
                                total_count=Signal::derive(move || state.get().total_count)
                                page_size=Signal::derive(move || state.get().page_size)
                                on_page_change=Callback::new(go_to_page)
                                on_page_size_change=Callback::new(change_page_size)
                                page_size_options=vec![25, 50, 100]
                            />
                        </div>
                        <div class="filter-panel-header__right">
                        </div>
                    </div>

                    <div class="filter-panel-content">
                        <Flex gap=FlexGap::Small align=FlexAlign::End>
                            <div style="flex: 1; max-width: 280px;">
                                <Input
                                    value=search_signal
                                    placeholder="Номер или заголовок..."
                                />
                            </div>
                            <select
                                class="thaw-input"
                                style="width: 160px;"
                                prop:value=move || type_signal.get()
                                on:change=move |ev| type_signal.set(event_target_value(&ev))
                            >
                                <option value="">"Все типы"</option>
                                {TicketType::all().iter().map(|t| view! {
                                    <option value=t.as_str()>{t.label_ru()}</option>
                                }).collect_view()}
                            </select>
                            <select
                                class="thaw-input"
                                style="width: 160px;"
                                prop:value=move || status_signal.get()
                                on:change=move |ev| status_signal.set(event_target_value(&ev))
                            >
                                <option value="">"Все статусы"</option>
                                {TicketStatus::all().iter().map(|s| view! {
                                    <option value=s.as_str()>{s.label_ru()}</option>
                                }).collect_view()}
                            </select>
                            <Button
                                appearance=ButtonAppearance::Primary
                                on_click=move |_| apply_filters()
                                disabled=Signal::derive(move || loading.get())
                            >
                                "Найти"
                            </Button>
                            <Button
                                appearance=ButtonAppearance::Secondary
                                on_click=move |_| reset_filters()
                            >
                                "Сбросить"
                            </Button>
                        </Flex>
                    </div>
                </div>

                <div class="table-wrapper">
                    <TableCrosshairHighlight table_id=TABLE_ID.to_string() />

                    <Table attr:id=TABLE_ID attr:style="width: 100%;">
                        <TableHeader>
                            <TableRow>
                                <TableHeaderCell resizable=false class="resizable" min_width=90.0>
                                    "Номер"
                                </TableHeaderCell>
                                <TableHeaderCell resizable=false class="resizable" min_width=110.0>
                                    "Тип"
                                </TableHeaderCell>
                                <TableHeaderCell resizable=false class="resizable" min_width=240.0>
                                    "Заголовок"
                                </TableHeaderCell>
                                <TableHeaderCell resizable=false class="resizable" min_width=110.0>
                                    "Статус"
                                </TableHeaderCell>
                                <TableHeaderCell resizable=false class="resizable" min_width=100.0>
                                    "Приоритет"
                                </TableHeaderCell>
                                <TableHeaderCell resizable=false class="resizable" min_width=120.0>
                                    "Автор"
                                </TableHeaderCell>
                                <TableHeaderCell resizable=false class="resizable" min_width=120.0>
                                    "Исполнитель"
                                </TableHeaderCell>
                                <TableHeaderCell resizable=false class="resizable" min_width=110.0>
                                    "Срок"
                                </TableHeaderCell>
                                <TableHeaderCell resizable=false class="resizable" min_width=130.0>
                                    "Обновлён"
                                </TableHeaderCell>
                            </TableRow>
                        </TableHeader>

                        <TableBody>
                            <For
                                each=move || state.get().items
                                key=|t| format!("{}:{}", t.id, t.updated_at)
                                children=move |ticket| {
                                    let id_for_click = ticket.id.clone();
                                    let code_for_click = ticket.code.clone();
                                    let updated = format_ts(&ticket.updated_at);
                                    let deadline = format_deadline(&ticket.deadline);
                                    let comment_count = ticket.comment_count;
                                    let attachment_count = ticket.attachment_count;
                                    view! {
                                        <TableRow>
                                            <TableCell>
                                                <TableCellLayout>
                                                    <span
                                                        class="table__link"
                                                        on:click=move |_| open_ticket_details(id_for_click.clone(), code_for_click.clone())
                                                    >
                                                        {ticket.code.clone()}
                                                    </span>
                                                </TableCellLayout>
                                            </TableCell>
                                            <TableCell>
                                                <TableCellLayout>
                                                    <span class=ticket_type_badge_class(ticket.ticket_type)>
                                                        {ticket.ticket_type.label_ru()}
                                                    </span>
                                                </TableCellLayout>
                                            </TableCell>
                                            <TableCell>
                                                <TableCellLayout truncate=true>
                                                    {ticket.title.clone()}
                                                    {if comment_count > 0 || attachment_count > 0 {
                                                        Some(view! {
                                                            <span class="sys-tickets__counters">
                                                                {(comment_count > 0).then(|| view! {
                                                                    <span class="sys-tickets__counter">
                                                                        {icon("message-circle")}
                                                                        {comment_count.to_string()}
                                                                    </span>
                                                                })}
                                                                {(attachment_count > 0).then(|| view! {
                                                                    <span class="sys-tickets__counter">
                                                                        {icon("paperclip")}
                                                                        {attachment_count.to_string()}
                                                                    </span>
                                                                })}
                                                            </span>
                                                        })
                                                    } else {
                                                        None
                                                    }}
                                                </TableCellLayout>
                                            </TableCell>
                                            <TableCell>
                                                <TableCellLayout>
                                                    <span class=ticket_status_badge_class(ticket.status)>
                                                        {ticket.status.label_ru()}
                                                    </span>
                                                </TableCellLayout>
                                            </TableCell>
                                            <TableCell>
                                                <TableCellLayout>
                                                    <span class=ticket_priority_badge_class(ticket.priority)>
                                                        {ticket.priority.label_ru()}
                                                    </span>
                                                </TableCellLayout>
                                            </TableCell>
                                            <TableCell>
                                                <TableCellLayout truncate=true>
                                                    {ticket.author_username.clone().unwrap_or_else(|| "-".to_string())}
                                                </TableCellLayout>
                                            </TableCell>
                                            <TableCell>
                                                <TableCellLayout truncate=true>
                                                    {ticket.assignee_username.clone().unwrap_or_else(|| "-".to_string())}
                                                </TableCellLayout>
                                            </TableCell>
                                            <TableCell>
                                                <TableCellLayout>{deadline}</TableCellLayout>
                                            </TableCell>
                                            <TableCell>
                                                <TableCellLayout>{updated}</TableCellLayout>
                                            </TableCell>
                                        </TableRow>
                                    }
                                }
                            />
                        </TableBody>
                    </Table>
                </div>
            </div>
        </PageFrame>
    }
}
