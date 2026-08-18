//! Уточняющие вопросы модели — прямо над полем ввода.
//!
//! Раньше они прятались в свёрнутой панели сверху и фактически не читались.
//! Здесь вопрос стоит там, где человек и так печатает ответ, а клик по варианту
//! не только фиксирует его в анкете, но и отправляет реплику в чат — модель
//! продолжает ход сразу, без «ну и что дальше».

use leptos::prelude::*;
use leptos::task::spawn_local;

use super::api::answer_intake_question;
use crate::shared::icons::icon;
use contracts::domain::a018_llm_chat::workspace::{ChatWorkspaceView, IntakeQuestion};

/// Одна строка бара: текст вопроса + варианты (или поле ввода).
#[component]
#[allow(non_snake_case)]
fn QuestionItem(
    chat_id: StoredValue<String>,
    question: IntakeQuestion,
    is_sending: RwSignal<bool>,
    on_answered: Callback<String>,
) -> impl IntoView {
    let qid = StoredValue::new(question.id.clone());
    let text = StoredValue::new(question.text.clone());
    let draft = RwSignal::new(String::new());
    let saving = RwSignal::new(false);
    let busy = move || saving.get() || is_sending.get();

    // Сначала фиксируем ответ в анкете (модель увидит его в intake.md),
    // и только потом отправляем реплику в чат.
    let submit = move |value: String| {
        if value.trim().is_empty() || saving.get_untracked() || is_sending.get_untracked() {
            return;
        }
        saving.set(true);
        let id = chat_id.get_value();
        let question_id = qid.get_value();
        spawn_local(async move {
            if answer_intake_question(&id, &question_id, &value)
                .await
                .is_ok()
            {
                // Составной текст, а не голое «да»: иначе история чата нечитабельна.
                on_answered.run(format!("Ответ на вопрос «{}»: {}", text.get_value(), value));
            }
            saving.set(false);
        });
    };

    let options = question.options.clone();
    let has_options = !options.is_empty();

    view! {
        <div class="chat-questions-bar__item">
            <div class="chat-questions-bar__text">{question.text.clone()}</div>
            <div class="chat-questions-bar__answers">
                {if has_options {
                    options
                        .into_iter()
                        .map(|option| {
                            let value = option.clone();
                            view! {
                                <button
                                    class="chat-workspace__option"
                                    disabled=move || busy()
                                    on:click=move |_| submit(value.clone())
                                >
                                    {option.clone()}
                                </button>
                            }
                        })
                        .collect_view()
                        .into_any()
                } else {
                    view! {
                        <input
                            class="chat-workspace__question-input"
                            placeholder="Ответ…"
                            prop:value=move || draft.get()
                            disabled=move || busy()
                            on:input=move |ev| draft.set(event_target_value(&ev))
                            on:keydown=move |ev| {
                                if ev.key() == "Enter" {
                                    ev.prevent_default();
                                    submit(draft.get_untracked());
                                }
                            }
                        />
                        <button
                            class="chat-workspace__option"
                            disabled=move || busy()
                            on:click=move |_| submit(draft.get_untracked())
                        >
                            "Ответить"
                        </button>
                    }
                    .into_any()
                }}
            </div>
        </div>
    }
}

/// Бар неотвеченных вопросов. Пусто — ничего не рисуем.
#[component]
#[allow(non_snake_case)]
pub fn ChatQuestionsBar(
    chat_id: StoredValue<String>,
    workspace: RwSignal<ChatWorkspaceView>,
    is_sending: RwSignal<bool>,
    /// Текст реплики, которую нужно отправить в чат от имени пользователя.
    on_answered: Callback<String>,
) -> impl IntoView {
    let pending = move || {
        workspace
            .get()
            .questions
            .into_iter()
            .filter(|q| q.answer.is_none())
            .collect::<Vec<_>>()
    };

    view! {
        <Show when=move || !pending().is_empty()>
            <div class="chat-questions-bar">
                <div class="chat-questions-bar__head">
                    <span class="chat-questions-bar__icon">{icon("info")}</span>
                    {move || format!("Ассистент уточняет ({})", pending().len())}
                </div>
                <For
                    each=pending
                    key=|q: &IntakeQuestion| q.id.clone()
                    let:question
                >
                    <QuestionItem
                        chat_id=chat_id
                        question=question
                        is_sending=is_sending
                        on_answered=on_answered
                    />
                </For>
            </div>
        </Show>
    }
}
