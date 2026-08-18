//! Глобальная кнопка «AI чат» в шапке приложения.
//!
//! По клику формирует контекст текущей страницы (по ключу активной вкладки) и
//! предлагает: создать новый чат с этим контекстом или добавить контекст в один
//! из уже открытых чатов.
//!
//! Сама цепочка создания чата живёт в `launch.rs`: тем же путём чат открывается
//! со страницы метрик, и дублировать выбор подключения незачем.

use crate::layout::global_context::AppGlobalContext;
use crate::shared::icons::icon;
use leptos::prelude::*;

use super::launch::{add_context, launch_chat_with_context, CHAT_DETAIL_PREFIX};

#[component]
#[allow(non_snake_case)]
pub fn AiChatHeaderButton() -> impl IntoView {
    let ctx = use_context::<AppGlobalContext>().expect("AppGlobalContext not found");
    let open = RwSignal::new(false);
    let busy = RwSignal::new(false);

    // Снимок текущей страницы (ключ + заголовок) на момент открытия меню.
    let current_page = move || -> Option<(String, String)> {
        let key = ctx.active.get()?;
        if key.starts_with(CHAT_DETAIL_PREFIX) {
            return None; // на странице самого чата контекст не нужен
        }
        let title = ctx
            .opened
            .get()
            .into_iter()
            .find(|t| t.key == key)
            .map(|t| t.title)
            .unwrap_or_else(|| key.clone());
        Some((key, title))
    };

    // Открытые чаты (для «добавить в чат»).
    let open_chats = move || -> Vec<(String, String)> {
        let mut chats: Vec<(String, String)> = ctx
            .opened
            .get()
            .into_iter()
            .filter(|t| t.key.starts_with(CHAT_DETAIL_PREFIX))
            .map(|t| {
                let chat_id = t
                    .key
                    .strip_prefix(CHAT_DETAIL_PREFIX)
                    .unwrap_or("")
                    .to_string();
                (chat_id, t.title)
            })
            .collect();

        // Вкладка, восстановленная из `?active=`, получает родовой заголовок «AI чат»,
        // поэтому несколько чатов в списке выглядели бы одинаково. Развести их можно
        // только началом идентификатора — но лишь там, где заголовки реально совпали.
        let titles: Vec<String> = chats.iter().map(|(_, t)| t.clone()).collect();
        for (chat_id, title) in chats.iter_mut() {
            if titles.iter().filter(|t| *t == title).count() > 1 {
                let short: String = chat_id.chars().take(6).collect();
                title.push_str(&format!(" · {short}"));
            }
        }
        chats
    };

    // Новый чат. Вход один: и вопрос по данным, и жалоба на работу программы идут
    // сюда — тему определяет роутер интентов по формулировке, а не пользователь
    // выбором пункта меню. Снимок навигации кладём всегда: для разбора «я был там,
    // нажал сюда, сломалось» он нужен, а аналитическому вопросу не мешает.
    let do_new_chat = move |page_key: String, label: String| {
        if busy.get_untracked() {
            return;
        }
        open.set(false);
        // Вопрос из шапки не подставляем: пользователь ещё не сформулировал его.
        launch_chat_with_context(ctx, page_key, label, None);
    };

    // Добавить контекст в существующий чат.
    let do_add_to = move |chat_id: String, page_key: String, label: String| {
        if busy.get_untracked() {
            return;
        }
        busy.set(true);
        open.set(false);
        wasm_bindgen_futures::spawn_local(async move {
            match add_context(&chat_id, &page_key, &label, false).await {
                Ok(()) => {
                    // Сигнал открытой странице чата перезагрузить ленту контекста.
                    let vkey = crate::domain::a018_llm_chat::ui::context_version_key(&chat_id);
                    let next = ctx
                        .get_form_state(&vkey)
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0)
                        + 1;
                    ctx.set_form_state(vkey, serde_json::Value::from(next));

                    let key = format!("{}{}", CHAT_DETAIL_PREFIX, chat_id);
                    ctx.activate_tab(&key);
                }
                Err(e) => leptos::logging::log!("AI чат: ошибка добавления контекста: {}", e),
            }
            busy.set(false);
        });
    };

    view! {
        <div style="position: relative; display: inline-flex;">
            <button
                class="app-header__icon-button"
                title="AI чат: контекст текущей страницы"
                on:click=move |_| open.update(|v| *v = !*v)
            >
                {icon("message-circle")}
            </button>

            <Show when=move || open.get()>
                // Бэкдроп для закрытия по клику снаружи
                <div
                    style="position: fixed; inset: 0; z-index: 1000;"
                    on:click=move |_| open.set(false)
                ></div>

                <div class="ai-chat-menu">
                    // Шапка меню — только вопрос и представление контекста. Ничего
                    // кликабельного: раньше первым пунктом стояло действие, и оно
                    // читалось как заголовок.
                    {move || match current_page() {
                        Some((_, label)) => {
                            let hint = label.clone();
                            view! {
                                <div class="ai-chat-menu__caption">
                                    "Что сделать с текущим контекстом?"
                                </div>
                                <div class="ai-chat-menu__context" title=hint>
                                    {label}
                                </div>
                            }.into_any()
                        }
                        None => view! {
                            <div class="ai-chat-menu__caption">"Открыть AI чат"</div>
                        }.into_any(),
                    }}

                    <div class="ai-chat-menu__divider"></div>

                    {move || {
                        let page = current_page();
                        let (pk, lbl) = page.clone().unwrap_or_default();
                        let (pk_new, lbl_new) = (pk.clone(), lbl.clone());
                        view! {
                            <button
                                class="ai-chat-menu__item"
                                disabled=busy
                                on:click=move |_| do_new_chat(pk_new.clone(), lbl_new.clone())
                            >
                                {icon("plus")}
                                <span>"Новый чат"</span>
                            </button>

                            {move || {
                                let chats = open_chats();
                                if chats.is_empty() || page.is_none() {
                                    return view! { <></> }.into_any();
                                }
                                let (pk, lbl) = (pk.clone(), lbl.clone());
                                view! {
                                    <div class="ai-chat-menu__divider"></div>
                                    <div class="ai-chat-menu__section">"Добавить в открытый чат"</div>
                                    {chats.into_iter().map(|(chat_id, title)| {
                                        let (pk, lbl) = (pk.clone(), lbl.clone());
                                        let hint = title.clone();
                                        view! {
                                            <button
                                                class="ai-chat-menu__item"
                                                title=hint
                                                disabled=busy
                                                on:click=move |_| do_add_to(chat_id.clone(), pk.clone(), lbl.clone())
                                            >
                                                {icon("message-circle")}
                                                <span class="ai-chat-menu__item-text">{title}</span>
                                            </button>
                                        }
                                    }).collect_view()}
                                }.into_any()
                            }}
                        }
                    }}
                </div>
            </Show>
        </div>
    }
}
