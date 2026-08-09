use crate::domain::a018_llm_chat::ui::details::LlmChatDetails;
use crate::domain::a042_agent_task::ui::list::{badge_color, format_ts};
use crate::layout::global_context::AppGlobalContext;
use crate::shared::api_utils::api_base;
use crate::shared::components::card_animated::CardAnimated;
use crate::shared::icons::icon;
use crate::shared::page_frame::PageFrame;
use crate::shared::page_standard::PAGE_CAT_DETAIL;
use contracts::domain::a042_agent_task::aggregate::{AgentTask, AgentTaskStatus};
use contracts::domain::common::AggregateId;
use gloo_net::http::Request;
use leptos::prelude::*;
use leptos::task::spawn_local;
use thaw::*;

#[component]
pub fn AgentTaskDetails(id: String, #[prop(into)] on_close: Callback<()>) -> impl IntoView {
    let tabs_ctx = use_context::<AppGlobalContext>().expect("AppGlobalContext not found");
    let (item, set_item) = signal::<Option<AgentTask>>(None);
    let (loading, set_loading) = signal(true);
    let (error, set_error) = signal::<Option<String>>(None);
    let active_tab = RwSignal::new("request".to_string());

    let id_store = StoredValue::new(id.clone());
    let load = move || {
        let id = id_store.get_value();
        let tabs_ctx = tabs_ctx.clone();
        spawn_local(async move {
            set_loading.set(true);
            set_error.set(None);
            let url = format!("{}/api/a042-agent-task/{}", api_base(), id);
            match Request::get(&url).send().await {
                Ok(resp) if resp.ok() => match resp.json::<AgentTask>().await {
                    Ok(payload) => {
                        tabs_ctx.update_tab_title(
                            &format!("a042_agent_task_details_{}", payload.base.id.as_string()),
                            &payload.base.description,
                        );
                        set_item.set(Some(payload));
                    }
                    Err(e) => set_error.set(Some(format!("Ошибка парсинга: {}", e))),
                },
                Ok(resp) => set_error.set(Some(format!("Ошибка сервера: HTTP {}", resp.status()))),
                Err(e) => set_error.set(Some(format!("Ошибка сети: {}", e))),
            }
            set_loading.set(false);
        });
    };

    Effect::new(move |_| load());

    let post_action = move |action: &'static str| {
        let id = id_store.get_value();
        spawn_local(async move {
            let url = format!("{}/api/a042-agent-task/{}/{}", api_base(), id, action);
            match Request::post(&url).send().await {
                Ok(resp) if resp.ok() => load(),
                // 409 — недопустимый переход; текст причины приходит телом,
                // показываем его, а не голый код.
                Ok(resp) => {
                    let status = resp.status();
                    let body = resp.text().await.unwrap_or_default();
                    let message = if body.trim().is_empty() {
                        format!("Ошибка действия: HTTP {}", status)
                    } else {
                        body
                    };
                    set_error.set(Some(message));
                }
                Err(e) => set_error.set(Some(format!("Ошибка сети: {}", e))),
            }
        });
    };

    view! {
        <PageFrame page_id="a042_agent_task_details" category=PAGE_CAT_DETAIL>
            <div class="page__header">
                <div class="page__header-left">
                    <h1 class="page__title">
                        {move || item.get().map(|i| i.base.description).unwrap_or_else(|| "Поручение".to_string())}
                    </h1>
                    {move || item.get().map(|i| {
                        view! {
                            <Badge
                                appearance=BadgeAppearance::Filled
                                color=badge_color(&i.status)
                            >
                                {i.status.display_name()}
                            </Badge>
                        }
                    })}
                </div>
                <div class="page__header-right">
                    <Space>
                        <Button
                            appearance=ButtonAppearance::Secondary
                            disabled=Signal::derive(move || {
                                item.get().map(|i| !i.is_open()).unwrap_or(true)
                            })
                            on_click=move |_| post_action("cancel")
                        >
                            "Отменить"
                        </Button>
                        <Button
                            appearance=ButtonAppearance::Primary
                            disabled=Signal::derive(move || {
                                item.get()
                                    .map(|i| !matches!(
                                        i.status,
                                        AgentTaskStatus::Failed | AgentTaskStatus::Cancelled
                                    ))
                                    .unwrap_or(true)
                            })
                            on_click=move |_| post_action("requeue")
                        >
                            "Повторить"
                        </Button>
                        <Button
                            appearance=ButtonAppearance::Secondary
                            on_click=move |_| load()
                            disabled=Signal::derive(move || loading.get())
                        >
                            "Обновить"
                        </Button>
                        <Button
                            appearance=ButtonAppearance::Secondary
                            size=ButtonSize::Medium
                            on_click=move |_| on_close.run(())
                        >
                            "Закрыть"
                        </Button>
                    </Space>
                </div>
            </div>

            <div class="page__tabs">
                <button
                    class="page__tab"
                    class:page__tab--active=move || active_tab.get() == "request"
                    on:click=move |_| active_tab.set("request".to_string())
                >
                    {icon("file-text")} "Задание"
                </button>
                <button
                    class="page__tab"
                    class:page__tab--active=move || active_tab.get() == "result"
                    on:click=move |_| active_tab.set("result".to_string())
                >
                    {icon("check-circle")} "Результат"
                </button>
                <button
                    class="page__tab"
                    class:page__tab--active=move || active_tab.get() == "chat"
                    on:click=move |_| active_tab.set("chat".to_string())
                >
                    {icon("message-square")} "Чат исполнителя"
                </button>
            </div>

            <div class="page__content">
                {move || {
                    if loading.get() {
                        return view! {
                            <Flex gap=FlexGap::Small style="align-items: center; padding: var(--spacing-4xl); justify-content: center;">
                                <Spinner />
                                <span>"Загрузка..."</span>
                            </Flex>
                        }.into_any();
                    }
                    if let Some(err) = error.get() {
                        return view! { <div class="alert alert--error">{err}</div> }.into_any();
                    }
                    let Some(current) = item.get() else {
                        return view! { <div>"Нет данных"</div> }.into_any();
                    };

                    match active_tab.get().as_str() {
                        "result" => result_tab(current).into_any(),
                        "chat" => chat_tab(current).into_any(),
                        _ => request_tab(current).into_any(),
                    }
                }}
            </div>
        </PageFrame>
    }
}

fn readonly_input(value: String) -> impl IntoView {
    view! { <Input value=RwSignal::new(value) attr:readonly=true /> }
}

fn or_dash(value: Option<String>) -> String {
    value
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| "—".to_string())
}

/// JSON-контекст показываем с отступами: строка в одну линию нечитаема, а именно
/// в ней чаще всего и лежит причина, почему исполнитель понял задачу иначе.
fn pretty_json(raw: &str) -> String {
    serde_json::from_str::<serde_json::Value>(raw)
        .ok()
        .and_then(|v| serde_json::to_string_pretty(&v).ok())
        .unwrap_or_else(|| raw.to_string())
}

fn request_tab(current: AgentTask) -> impl IntoView {
    let created_at = current
        .base
        .metadata
        .created_at
        .format("%d.%m.%Y %H:%M:%S")
        .to_string();
    let payload = current.payload_json.as_deref().map(pretty_json);

    view! {
        <div class="detail-grid">
            <div class="detail-grid__col">
                <CardAnimated delay_ms=0 nav_id="a042_agent_task_details_request_body">
                    <h4 class="details-section__title">"Постановка задачи"</h4>
                    <div style="display: grid; grid-template-columns: 1fr 1fr; gap: var(--spacing-sm);">
                        <div class="form__group">
                            <label class="form__label">"Код"</label>
                            {readonly_input(current.base.code)}
                        </div>
                        <div class="form__group">
                            <label class="form__label">"Статус"</label>
                            {readonly_input(current.status.display_name().to_string())}
                        </div>
                    </div>
                    <div class="form__group">
                        <label class="form__label">"Заголовок"</label>
                        {readonly_input(current.base.description)}
                    </div>
                    <div class="form__group">
                        <label class="form__label">"Задание исполнителю"</label>
                        <textarea class="form__control" rows="10" readonly>{current.request_text}</textarea>
                    </div>
                    {payload.map(|json| view! {
                        <div class="form__group">
                            <label class="form__label">"Структурированный контекст"</label>
                            <textarea class="form__control" rows="8" readonly>{json}</textarea>
                        </div>
                    })}
                </CardAnimated>
            </div>

            <div class="detail-grid__col">
                <CardAnimated delay_ms=80 nav_id="a042_agent_task_details_request_meta">
                    <h4 class="details-section__title">"Маршрут"</h4>
                    <div class="form__group">
                        <label class="form__label">"Специалист-исполнитель"</label>
                        {readonly_input(current.target_agent_type.display_name().to_string())}
                    </div>
                    <div style="display: grid; grid-template-columns: 1fr 1fr; gap: var(--spacing-sm);">
                        <div class="form__group">
                            <label class="form__label">"Глубина цепочки"</label>
                            {readonly_input(current.depth.to_string())}
                        </div>
                        <div class="form__group">
                            <label class="form__label">"Создано"</label>
                            {readonly_input(created_at)}
                        </div>
                    </div>
                    <div class="form__group">
                        <label class="form__label">"Родительское поручение"</label>
                        {readonly_input(or_dash(current.parent_task_ref))}
                    </div>
                    <div class="form__group">
                        <label class="form__label">"Чат-заказчик"</label>
                        {readonly_input(or_dash(current.requested_by_chat_ref))}
                    </div>
                    <div class="form__group">
                        <label class="form__label">"Агент-заказчик"</label>
                        {readonly_input(or_dash(current.requested_by_agent_ref))}
                    </div>
                    <div class="form__group">
                        <label class="form__label">"Пользователь-заказчик"</label>
                        {readonly_input(or_dash(current.requested_by_user_ref))}
                    </div>
                </CardAnimated>
            </div>
        </div>
    }
}

fn result_tab(current: AgentTask) -> impl IntoView {
    let attempts = format!("{}/{}", current.attempts, current.max_attempts);
    let started = or_dash(current.started_at.as_deref().map(format_ts));
    let finished = or_dash(current.finished_at.as_deref().map(format_ts));
    let next_attempt = or_dash(current.next_attempt_at.as_deref().map(format_ts));
    let error = current.error.clone();
    let result_text = current.result_text.clone();
    let is_done = current.status == AgentTaskStatus::Done;

    view! {
        <div class="detail-grid">
            <div class="detail-grid__col">
                <CardAnimated delay_ms=0 nav_id="a042_agent_task_details_result_body">
                    <h4 class="details-section__title">"Ответ исполнителя"</h4>
                    {error.map(|text| view! {
                        <div class="alert alert--error" style="margin-bottom: var(--spacing-md);">
                            {text}
                        </div>
                    })}
                    {match result_text {
                        Some(text) if !text.trim().is_empty() => view! {
                            <textarea class="form__control" rows="18" readonly>{text}</textarea>
                        }.into_any(),
                        _ => view! {
                            <p>
                                {if is_done {
                                    "Исполнитель вернул пустой ответ."
                                } else {
                                    "Результата ещё нет: поручение не исполнено."
                                }}
                            </p>
                        }.into_any(),
                    }}
                </CardAnimated>
            </div>

            <div class="detail-grid__col">
                <CardAnimated delay_ms=80 nav_id="a042_agent_task_details_result_meta">
                    <h4 class="details-section__title">"Прогон"</h4>
                    <div style="display: grid; grid-template-columns: 1fr 1fr; gap: var(--spacing-sm);">
                        <div class="form__group">
                            <label class="form__label">"Попыток"</label>
                            {readonly_input(attempts)}
                        </div>
                        <div class="form__group">
                            <label class="form__label">"Повтор не раньше"</label>
                            {readonly_input(next_attempt)}
                        </div>
                        <div class="form__group">
                            <label class="form__label">"Начато"</label>
                            {readonly_input(started)}
                        </div>
                        <div class="form__group">
                            <label class="form__label">"Завершено"</label>
                            {readonly_input(finished)}
                        </div>
                    </div>
                    <div class="form__group">
                        <label class="form__label">"Сессия прогона"</label>
                        {readonly_input(or_dash(current.claim_session_id))}
                    </div>
                    <div class="form__group">
                        <label class="form__label">"Сотрудник-исполнитель"</label>
                        {readonly_input(or_dash(current.executor_agent_ref))}
                    </div>
                    <div class="form__group">
                        <label class="form__label">"Чат исполнения"</label>
                        {readonly_input(or_dash(current.result_chat_ref))}
                    </div>
                    <div class="form__group">
                        <label class="form__label">"Артефакт"</label>
                        {readonly_input(or_dash(current.result_artifact_ref))}
                    </div>
                </CardAnimated>
            </div>
        </div>
    }
}

fn chat_tab(current: AgentTask) -> impl IntoView {
    view! {
        <div class="detail-grid">
            <div class="detail-grid__col" style="grid-column: 1 / -1;">
                <CardAnimated delay_ms=0 nav_id="a042_agent_task_details_chat_main">
                    <h4 class="details-section__title">"Диалог исполнения"</h4>
                    {match current.result_chat_ref {
                        Some(chat_id) => view! {
                            <LlmChatDetails id=chat_id on_close=Callback::new(|_| {}) />
                        }.into_any(),
                        None => view! {
                            <p>"Поручение ещё не бралось в работу — диалога исполнения нет."</p>
                        }.into_any(),
                    }}
                </CardAnimated>
            </div>
        </div>
    }
}
