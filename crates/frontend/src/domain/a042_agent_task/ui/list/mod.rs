use crate::layout::global_context::AppGlobalContext;
use crate::shared::api_utils::api_base;
use crate::shared::components::close_page_button::ClosePageButton;
use crate::shared::components::pagination_controls::PaginationControls;
use crate::shared::components::table::TableCrosshairHighlight;
use crate::shared::components::ui::badge::Badge as UiBadge;
use crate::shared::icons::icon;
use crate::shared::page_frame::PageFrame;
use crate::shared::page_standard::PAGE_CAT_LIST;
use crate::shared::table_utils::init_column_resize;
use crate::system::tasks::ui::RunTaskButton;
use contracts::domain::a042_agent_task::aggregate::{AgentTask, AgentTaskStatus};
use contracts::domain::common::AggregateId;
use gloo_net::http::Request;
use leptos::prelude::*;
use leptos::task::spawn_local;
use serde::Deserialize;
use thaw::*;

#[derive(Debug, Clone, Deserialize)]
struct AgentTaskListResponse {
    items: Vec<AgentTask>,
    total: u64,
    total_pages: usize,
}

const TABLE_ID: &str = "a042-agent-task-table";
const COLUMN_WIDTHS_KEY: &str = "a042_agent_task_column_widths";

#[component]
pub fn AgentTaskList() -> impl IntoView {
    let tabs_store =
        leptos::context::use_context::<AppGlobalContext>().expect("AppGlobalContext not found");
    let (items, set_items) = signal::<Vec<AgentTask>>(Vec::new());
    let (loading, set_loading) = signal(false);
    let (error, set_error) = signal::<Option<String>>(None);
    let selected_status = RwSignal::new(String::new());
    let selected_agent_type = RwSignal::new(String::new());
    let search_query = RwSignal::new(String::new());
    let page = RwSignal::new(0usize);
    let page_size = RwSignal::new(100usize);
    let total_count = RwSignal::new(0usize);
    let total_pages = RwSignal::new(1usize);
    let sort_field = RwSignal::new("created_at".to_string());
    let sort_ascending = RwSignal::new(false);
    let (is_filter_expanded, set_is_filter_expanded) = signal(false);

    let load_items = move || {
        spawn_local(async move {
            set_loading.set(true);
            set_error.set(None);
            let status = selected_status.get_untracked();
            let agent_type = selected_agent_type.get_untracked();
            let query = search_query.get_untracked();
            let limit = page_size.get_untracked();
            let offset = page.get_untracked() * limit;
            let mut url = format!(
                "{}/api/a042-agent-task/list?limit={}&offset={}&sort_by={}&sort_desc={}",
                api_base(),
                limit,
                offset,
                urlencoding::encode(&sort_field.get_untracked()),
                !sort_ascending.get_untracked()
            );
            if !status.is_empty() {
                url.push_str(&format!("&status={}", urlencoding::encode(&status)));
            }
            if !agent_type.is_empty() {
                url.push_str(&format!(
                    "&target_agent_type={}",
                    urlencoding::encode(&agent_type)
                ));
            }
            if !query.trim().is_empty() {
                url.push_str(&format!("&q={}", urlencoding::encode(&query)));
            }
            match Request::get(&url).send().await {
                Ok(resp) if resp.ok() => match resp.json::<AgentTaskListResponse>().await {
                    Ok(payload) => {
                        total_count.set(payload.total as usize);
                        total_pages.set(payload.total_pages.max(1));
                        set_items.set(payload.items);
                    }
                    Err(e) => set_error.set(Some(format!("Ошибка парсинга: {}", e))),
                },
                Ok(resp) => set_error.set(Some(format!("Ошибка сервера: HTTP {}", resp.status()))),
                Err(e) => set_error.set(Some(format!("Ошибка сети: {}", e))),
            }
            set_loading.set(false);
        });
    };

    Effect::new(move |_| {
        let _ = selected_status.get();
        let _ = selected_agent_type.get();
        load_items();
    });

    let resize_initialized = StoredValue::new(false);
    Effect::new(move |_| {
        if !resize_initialized.get_value() {
            resize_initialized.set_value(true);
            spawn_local(async move {
                gloo_timers::future::TimeoutFuture::new(100).await;
                init_column_resize(TABLE_ID, COLUMN_WIDTHS_KEY);
            });
        }
    });

    let open_detail = move |id: String, title: String| {
        tabs_store.open_tab(&format!("a042_agent_task_details_{}", id), &title);
    };

    let go_to_page = move |new_page: usize| {
        page.set(new_page);
        load_items();
    };

    let change_page_size = move |new_size: usize| {
        page_size.set(new_size);
        page.set(0);
        load_items();
    };

    let toggle_sort = move |field: &'static str| {
        if sort_field.get_untracked() == field {
            sort_ascending.update(|value| *value = !*value);
        } else {
            sort_field.set(field.to_string());
            sort_ascending.set(true);
        }
        page.set(0);
        load_items();
    };

    let sort_indicator = move |field: &'static str| -> &'static str {
        if sort_field.get() != field {
            ""
        } else if sort_ascending.get() {
            " ↑"
        } else {
            " ↓"
        }
    };

    let active_filters_count = Signal::derive(move || {
        let mut count = 0;
        if !selected_status.get().is_empty() {
            count += 1;
        }
        if !selected_agent_type.get().is_empty() {
            count += 1;
        }
        if !search_query.get().trim().is_empty() {
            count += 1;
        }
        count
    });

    view! {
        <PageFrame page_id="a042_agent_task--list" category=PAGE_CAT_LIST>
            <div class="page__header">
                <div class="page__header-left">
                    <h1 class="page__title">"Поручения AI-сотрудникам"</h1>
                    <UiBadge variant="primary".to_string()>
                        {move || total_count.get().to_string()}
                    </UiBadge>
                </div>
                <div class="page__header-right">
                    <Space>
                        <RunTaskButton
                            task_code="task029-agent-task-runner".to_string()
                            label="Выполнить очередь".to_string()
                        />
                        <Button
                            appearance=ButtonAppearance::Primary
                            on_click=move |_| load_items()
                            disabled=Signal::derive(move || loading.get())
                        >
                            {move || if loading.get() { "Загрузка..." } else { "Обновить" }}
                        </Button>
                        <ClosePageButton />
                    </Space>
                </div>
            </div>

            <div class="page__content">
                <div class="filter-panel">
                    <div class="filter-panel-header">
                        <div
                            class="filter-panel-header__left"
                            on:click=move |_| set_is_filter_expanded.update(|value| *value = !*value)
                        >
                            {icon("filter")}
                            <span class="filter-panel__title">"Фильтры"</span>
                            {move || {
                                let count = active_filters_count.get();
                                if count > 0 {
                                    view! { <span class="filter-panel__badge">{count}</span> }.into_any()
                                } else {
                                    view! { <></> }.into_any()
                                }
                            }}
                        </div>
                        <div class="filter-panel-header__center">
                            <PaginationControls
                                current_page=Signal::derive(move || page.get())
                                total_pages=Signal::derive(move || total_pages.get())
                                total_count=Signal::derive(move || total_count.get())
                                page_size=Signal::derive(move || page_size.get())
                                on_page_change=Callback::new(go_to_page)
                                on_page_size_change=Callback::new(change_page_size)
                                page_size_options=vec![50, 100, 200, 500]
                            />
                        </div>
                    </div>

                    <Show when=move || is_filter_expanded.get()>
                        <div class="filter-panel-content">
                            <Flex gap=FlexGap::Small align=FlexAlign::End>
                                <div style="width: 200px;">
                                    <Flex vertical=true gap=FlexGap::Small>
                                        <Label>"Статус:"</Label>
                                        <Select value=selected_status>
                                            <option value="">"Все статусы"</option>
                                            <option value="pending">"Ожидание"</option>
                                            <option value="processing">"Исполняется"</option>
                                            <option value="done">"Готово"</option>
                                            <option value="failed">"Ошибка"</option>
                                            <option value="cancelled">"Отменено"</option>
                                        </Select>
                                    </Flex>
                                </div>
                                <div style="width: 230px;">
                                    <Flex vertical=true gap=FlexGap::Small>
                                        <Label>"Специалист:"</Label>
                                        <Select value=selected_agent_type>
                                            <option value="">"Все специализации"</option>
                                            <option value="business_analyst">"Бизнес-аналитик"</option>
                                            <option value="sales_analyst">"Аналитик продаж"</option>
                                            <option value="marketer">"Маркетолог"</option>
                                            <option value="financier">"Финансист"</option>
                                            <option value="system_admin">"Системный администратор"</option>
                                            <option value="kb_admin">"Администратор базы знаний"</option>
                                            <option value="plugin_admin">"Разработчик"</option>
                                            <option value="tester">"Тестировщик"</option>
                                        </Select>
                                    </Flex>
                                </div>
                                <div style="flex: 1; max-width: 420px;">
                                    <Flex vertical=true gap=FlexGap::Small>
                                        <Label>"Поиск:"</Label>
                                        <Input
                                            value=search_query
                                            placeholder="Заголовок, код, постановка задачи..."
                                            on:change=move |_| {
                                                page.set(0);
                                                load_items();
                                            }
                                        />
                                    </Flex>
                                </div>
                            </Flex>
                        </div>
                    </Show>
                </div>

                {move || error.get().map(|msg| view! { <div class="alert alert--error">{msg}</div> })}

                <div class="table-wrapper">
                    <TableCrosshairHighlight table_id=TABLE_ID.to_string() />
                    <Table attr:id=TABLE_ID attr:style="width: 100%; min-width: 1200px;">
                        <TableHeader>
                            <TableRow>
                                <TableHeaderCell resizable=false min_width=280.0 class="resizable">
                                    "Заголовок"
                                </TableHeaderCell>
                                <TableHeaderCell resizable=false min_width=180.0 class="resizable">
                                    <div class="table__sortable-header" on:click=move |_| toggle_sort("target_agent_type")>
                                        "Специалист" {move || sort_indicator("target_agent_type")}
                                    </div>
                                </TableHeaderCell>
                                <TableHeaderCell resizable=false min_width=130.0 class="resizable">
                                    <div class="table__sortable-header" on:click=move |_| toggle_sort("status")>
                                        "Статус" {move || sort_indicator("status")}
                                    </div>
                                </TableHeaderCell>
                                <TableHeaderCell resizable=false min_width=100.0 class="resizable">"Попыток"</TableHeaderCell>
                                <TableHeaderCell resizable=false min_width=160.0 class="resizable">
                                    <div class="table__sortable-header" on:click=move |_| toggle_sort("created_at")>
                                        "Создано" {move || sort_indicator("created_at")}
                                    </div>
                                </TableHeaderCell>
                                <TableHeaderCell resizable=false min_width=160.0 class="resizable">
                                    <div class="table__sortable-header" on:click=move |_| toggle_sort("finished_at")>
                                        "Завершено" {move || sort_indicator("finished_at")}
                                    </div>
                                </TableHeaderCell>
                                <TableHeaderCell resizable=false min_width=320.0 class="resizable">"Результат"</TableHeaderCell>
                            </TableRow>
                        </TableHeader>
                        <TableBody>
                            {move || if loading.get() {
                                view! {
                                    <TableRow>
                                        <TableCell>
                                            <Flex gap=FlexGap::Small style="align-items: center; justify-content: center; padding: var(--spacing-xl);">
                                                <Spinner />
                                                <span>"Загрузка..."</span>
                                            </Flex>
                                        </TableCell>
                                        <TableCell></TableCell>
                                        <TableCell></TableCell>
                                        <TableCell></TableCell>
                                        <TableCell></TableCell>
                                        <TableCell></TableCell>
                                        <TableCell></TableCell>
                                    </TableRow>
                                }.into_any()
                            } else if items.get().is_empty() {
                                view! {
                                    <TableRow>
                                        <TableCell>"Нет поручений"</TableCell>
                                        <TableCell></TableCell>
                                        <TableCell></TableCell>
                                        <TableCell></TableCell>
                                        <TableCell></TableCell>
                                        <TableCell></TableCell>
                                        <TableCell></TableCell>
                                    </TableRow>
                                }.into_any()
                            } else {
                                view! {
                                    <For
                                        each=move || items.get()
                                        key=|item| item.base.id.as_string()
                                        children=move |item| {
                                            let id = item.base.id.as_string();
                                            let title = item.base.description.clone();
                                            let title_for_click = title.clone();
                                            let attempts = format!("{}/{}", item.attempts, item.max_attempts);
                                            let finished = item
                                                .finished_at
                                                .as_deref()
                                                .map(format_ts)
                                                .unwrap_or_else(|| "—".to_string());
                                            // У провалившегося поручения интересен не пустой
                                            // результат, а причина — показываем её в той же колонке.
                                            let outcome = item
                                                .result_text
                                                .clone()
                                                .or_else(|| item.error.clone())
                                                .unwrap_or_default();
                                            view! {
                                                <TableRow>
                                                    <TableCell>
                                                        <a
                                                            href="#"
                                                            class="table__link"
                                                            on:click=move |e| {
                                                                e.prevent_default();
                                                                open_detail(id.clone(), title_for_click.clone());
                                                            }
                                                        >
                                                            {title}
                                                        </a>
                                                    </TableCell>
                                                    <TableCell>{item.target_agent_type.display_name()}</TableCell>
                                                    <TableCell>{item.status.display_name()}</TableCell>
                                                    <TableCell>{attempts}</TableCell>
                                                    <TableCell>{item.base.metadata.created_at.format("%d.%m.%Y %H:%M").to_string()}</TableCell>
                                                    <TableCell>{finished}</TableCell>
                                                    <TableCell>
                                                        <TableCellLayout truncate=true>
                                                            {outcome}
                                                        </TableCellLayout>
                                                    </TableCell>
                                                </TableRow>
                                            }
                                        }
                                    />
                                }.into_any()
                            }}
                        </TableBody>
                    </Table>
                </div>
            </div>
        </PageFrame>
    }
}

/// RFC3339 из БД → человеческий формат. Непарсируемое значение показываем как есть,
/// а не прячем: пустая ячейка вместо кривой даты скрывает проблему.
pub fn format_ts(value: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|dt| {
            dt.with_timezone(&chrono::Local)
                .format("%d.%m.%Y %H:%M")
                .to_string()
        })
        .unwrap_or_else(|_| value.to_string())
}

/// Цвет бейджа статуса. Исчерпывающий match: новый статус потребует правки здесь.
pub fn badge_color(status: &AgentTaskStatus) -> BadgeColor {
    match status {
        AgentTaskStatus::Done => BadgeColor::Success,
        AgentTaskStatus::Processing => BadgeColor::Informative,
        AgentTaskStatus::Pending => BadgeColor::Warning,
        AgentTaskStatus::Failed => BadgeColor::Danger,
        AgentTaskStatus::Cancelled => BadgeColor::Subtle,
    }
}
