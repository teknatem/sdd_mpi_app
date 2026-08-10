//! Глобальная кнопка «AI чат» в шапке приложения.
//!
//! По клику формирует контекст текущей страницы (по ключу активной вкладки) и
//! предлагает: создать новый чат с этим контекстом или добавить контекст в один
//! из уже открытых чатов.

use crate::layout::global_context::AppGlobalContext;
use crate::shared::api_utils::api_base;
use crate::shared::icons::icon;
use leptos::prelude::*;

const CHAT_DETAIL_PREFIX: &str = "a018_llm_chat_details_";

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
        busy.set(true);
        open.set(false);
        wasm_bindgen_futures::spawn_local(async move {
            let result: Result<String, String> = async {
                let (agent_id, model) = fetch_default_agent().await?;
                let desc = derive_title(&label);
                let chat_id = create_chat(&desc, &agent_id, &model).await?;
                // Контекст не обязателен: с вкладки без полезного контекста чат тоже
                // должен открываться.
                if !page_key.is_empty() {
                    add_context(&chat_id, &page_key, &label, true).await?;
                }
                Ok(chat_id)
            }
            .await;
            match result {
                Ok(chat_id) => {
                    let key = format!("{}{}", CHAT_DETAIL_PREFIX, chat_id);
                    ctx.open_tab(&key, "AI чат");
                }
                Err(e) => leptos::logging::log!("AI чат: ошибка создания: {}", e),
            }
            busy.set(false);
        });
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

/// Заголовок чата из заголовка страницы.
fn derive_title(label: &str) -> String {
    let l = label.trim();
    if l.is_empty() {
        return "AI чат".to_string();
    }
    let max = 60;
    let chars: Vec<char> = l.chars().collect();
    let base = if chars.len() > max {
        let t: String = chars.into_iter().take(max).collect();
        format!("{}…", t.trim_end())
    } else {
        l.to_string()
    };
    format!("AI: {}", base)
}

// ─── API ─────────────────────────────────────────────────────────────────────

async fn http_request(method: &str, url: &str, body: Option<String>) -> Result<String, String> {
    use wasm_bindgen::JsCast;
    use web_sys::{Request, RequestInit, RequestMode, Response};

    let opts = RequestInit::new();
    opts.set_method(method);
    opts.set_mode(RequestMode::Cors);
    if let Some(b) = &body {
        opts.set_body(&wasm_bindgen::JsValue::from_str(b));
    }

    let request = Request::new_with_str_and_init(url, &opts).map_err(|e| format!("{e:?}"))?;
    request
        .headers()
        .set("Accept", "application/json")
        .map_err(|e| format!("{e:?}"))?;
    if body.is_some() {
        request
            .headers()
            .set("Content-Type", "application/json")
            .map_err(|e| format!("{e:?}"))?;
    }

    let window = web_sys::window().ok_or_else(|| "no window".to_string())?;
    let resp_value = wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("{e:?}"))?;
    let resp: Response = resp_value.dyn_into().map_err(|e| format!("{e:?}"))?;
    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }
    let text = wasm_bindgen_futures::JsFuture::from(resp.text().map_err(|e| format!("{e:?}"))?)
        .await
        .map_err(|e| format!("{e:?}"))?;
    text.as_string().ok_or_else(|| "bad text".to_string())
}

/// Подключение по умолчанию: основное (is_primary), иначе первое. Возвращает (id, model).
async fn fetch_default_agent() -> Result<(String, String), String> {
    let url = format!("{}/api/a038-llm-connection", api_base());
    let text = http_request("GET", &url, None).await?;
    let agents: Vec<serde_json::Value> = serde_json::from_str(&text).map_err(|e| format!("{e}"))?;
    if agents.is_empty() {
        return Err("Нет доступных LLM-подключений".to_string());
    }
    let chosen = agents
        .iter()
        .find(|a| {
            a.get("is_primary")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        })
        .or_else(|| agents.first())
        .unwrap();
    let id = chosen
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "agent without id".to_string())?
        .to_string();
    let model = chosen
        .get("model_name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    Ok((id, model))
}

async fn create_chat(description: &str, agent_id: &str, model: &str) -> Result<String, String> {
    let model_value = if model.trim().is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::Value::String(model.to_string())
    };
    let dto = serde_json::json!({
        "id": serde_json::Value::Null,
        "code": serde_json::Value::Null,
        "description": description,
        "comment": serde_json::Value::Null,
        "agent_id": agent_id,
        "model_name": model_value,
    });
    let url = format!("{}/api/a018-llm-chat", api_base());
    let text = http_request("POST", &url, Some(dto.to_string())).await?;
    let v: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("{e}"))?;
    v.get("id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "No chat id in response".to_string())
}

async fn add_context(
    chat_id: &str,
    page_key: &str,
    label: &str,
    with_session_snapshot: bool,
) -> Result<(), String> {
    let dto = serde_json::json!({
        "page_key": page_key,
        "label": label,
        "with_session_snapshot": with_session_snapshot,
    });
    let url = format!("{}/api/a018-llm-chat/{}/context", api_base(), chat_id);
    http_request("POST", &url, Some(dto.to_string())).await?;
    Ok(())
}
