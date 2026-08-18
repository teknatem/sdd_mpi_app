//! LLM Chat Details - View Component
//!
//! Унифицирован с detail-страницами: PageFrame, page__header, page__content.
//! Агент отображается по имени (agent_name из API), а не по UUID.

use super::api::{
    cancel_job, delete_chat, delete_pending_attachment, fetch_attachment_object_url, fetch_chat,
    fetch_chat_context, fetch_connection_model_capabilities, fetch_messages, fetch_workspace,
    poll_until_done, send_message, set_chat_model, set_rating, JobProgress, PollOutcome,
};
use super::artifact_card::ArtifactCard;
use super::view_model::LlmChatDetailsVm;

/// Предопределённое сообщение для кнопки «Диагностика»: модель разбирает текущий диалог
/// текстом, без вызова инструментов. Комментарий пользователя (если есть) дописывается следом.
const DIAGNOSTIC_PROMPT: &str = "Проведи диагностику этого диалога. Только ТЕКСТОВЫЙ разбор — \
НЕ вызывай инструменты и ничего не создавай/не пересоздавай.\n\nПроанализируй:\n\
1) что хотел пользователь;\n2) какие шаги и инструменты выполнялись, какие ошибки встречались;\n\
3) что не получилось и почему (корневая причина);\n4) конкретные следующие шаги для решения.\n\n\
Ответь кратко и по делу на русском.";

/// Предопределённое сообщение для кнопки «В базу знаний»: модель выжимает из диалога
/// одну бизнес-статью и заводит её черновиком. `use_skill` обязателен — запись в базу
/// намеренно не входит в базовый набор инструментов.
const KB_ARTICLE_PROMPT: &str = "Оформи знание из этого диалога в статью базы знаний.\n\n\
Порядок:\n\
1) вызови use_skill(\"kb-authoring\");\n\
2) проверь через search_knowledge, нет ли уже такой статьи (есть — предложи дополнить её, \
указав replaces);\n\
3) возьми теги из list_kb_vocabulary;\n\
4) вызови kb_propose_article.\n\n\
Выдели ОДНУ устойчивую находку: вывод, правило или объяснение расхождения. Пиши только \
бизнес-знание — что означает показатель и что с ним делать. НЕ включай SQL, схемы таблиц, \
имена полей и пути API: проверка — если фраза перестанет быть верной после рефакторинга схемы, \
её нельзя писать в базу знаний. Разовые числа за период тоже не пиши, они протухнут.\n\n\
В конце сошлись на созданную статью строкой kb://article/<id> и скажи, что она ждёт проверки \
человеком.";
use super::prefs::ChatUiPrefs;
use super::questions_bar::ChatQuestionsBar;
use super::settings_dialog::ChatSettingsDialog;
use super::tool_calls_trace::ToolCallsTrace;
use super::workspace_drawer::ChatWorkspaceDrawer;
use crate::domain::a018_llm_chat::ui::pending_first_message_key;
use crate::layout::global_context::AppGlobalContext;
use crate::shared::components::hint_link::HintLink;
use crate::shared::components::more_actions_menu::{use_more_actions_close, MoreActionsMenu};
use crate::shared::date_utils::{format_datetime_utc_local, format_utc_local};
use crate::shared::icons::icon;
use crate::shared::knowledge_base::links::KbLinkedText;
use crate::shared::markdown::Markdown;
use crate::shared::page_frame::PageFrame;
use crate::shared::page_standard::PAGE_CAT_DETAIL;
use crate::shared::screenshot_editor::{
    is_editable_image_type, PendingScreenshot, ScreenshotEditor,
};
use crate::shared::speech::{DictationButton, DictationDiagnostics};
use contracts::domain::a018_llm_chat::aggregate::{
    ChatRole, LlmChatAttachmentSummary, LlmChatMessage,
};
use contracts::domain::a018_llm_chat::context::ContextPackageSummary;
use contracts::domain::a018_llm_chat::workspace::ChatWorkspaceView;
use contracts::domain::common::AggregateId;
use leptos::prelude::*;
use thaw::*;
use uuid::Uuid;

/// Один элемент ленты чата: либо сообщение, либо событие прикрепления контекста.
/// Оба сортируются по времени создания для хронологического показа.
enum FeedItem {
    Message(LlmChatMessage),
    Context(ContextPackageSummary),
}

struct FeedRow {
    ts: chrono::DateTime<chrono::Utc>,
    key: String,
    item: FeedItem,
}

#[component]
fn AttachmentImage(chat_id: String, attachment: LlmChatAttachmentSummary) -> impl IntoView {
    let object_url = RwSignal::new(None::<String>);
    let load_error = RwSignal::new(false);
    let attachment_id = attachment.id.to_string();

    Effect::new(move |_| {
        let chat_id = chat_id.clone();
        let attachment_id = attachment_id.clone();
        wasm_bindgen_futures::spawn_local(async move {
            match fetch_attachment_object_url(&chat_id, &attachment_id).await {
                Ok(url) => object_url.set(Some(url)),
                Err(_) => load_error.set(true),
            }
        });
    });
    on_cleanup(move || {
        if let Some(url) = object_url.get_untracked() {
            let _ = web_sys::Url::revoke_object_url(&url);
        }
    });

    let filename = attachment.filename;
    view! {
        {move || {
            if let Some(url) = object_url.get() {
                let open_url = url.clone();
                view! {
                    <button
                        type="button"
                        title="Открыть изображение"
                        style="border:0;background:none;padding:0;cursor:pointer;line-height:0;"
                        on:click=move |_| {
                            if let Some(window) = web_sys::window() {
                                let _ = window.open_with_url_and_target(&open_url, "_blank");
                            }
                        }
                    >
                        <img
                            src=url
                            alt=filename.clone()
                            style="display:block; width: min(320px, 100%); max-height: 220px; object-fit: contain; border: 1px solid var(--colorNeutralStroke2); border-radius: 6px; background: var(--colorNeutralBackground2);"
                        />
                    </button>
                }.into_any()
            } else if load_error.get() {
                view! { <span style="font-size:12px;color:var(--colorPaletteRedForeground1);">"Не удалось загрузить изображение"</span> }.into_any()
            } else {
                view! { <span style="font-size:12px;opacity:.65;">"Загрузка изображения…"</span> }.into_any()
            }
        }}
    }
}

/// Левый «жёлоб» строки ленты: аватар блока, имя автора и время (до секунд),
/// выровненные по левой границе блока.
#[allow(non_snake_case)]
fn FeedGutter(avatar: &'static str, author: &'static str, time: String) -> impl IntoView {
    view! {
        <div style="flex: 0 0 104px; display: flex; align-items: flex-start; gap: 8px;">
            <div style="flex: 0 0 auto; line-height: 0; margin-top: 1px;">{icon(avatar)}</div>
            <div style="display: flex; flex-direction: column; gap: 2px; text-align: left; min-width: 0;">
                <div style="font-size: 11px; font-weight: 600; letter-spacing: .02em; opacity: 0.6;">
                    {author}
                </div>
                <div style="font-size: 11px; opacity: 0.45; font-variant-numeric: tabular-nums;">
                    {time}
                </div>
            </div>
        </div>
    }
}

/// Строка сообщения чата (пользователь / ассистент).
///
/// `prefs` решает, показывать ли технические поля (мета, токены, вызовы
/// инструментов, предупреждения о навыках) — по умолчанию они скрыты.
#[allow(non_snake_case)]
fn MessageRow(msg: LlmChatMessage, prefs: RwSignal<ChatUiPrefs>) -> impl IntoView {
    let is_user = matches!(msg.role, ChatRole::User);
    let tokens = msg.tokens_used;
    let model = msg.model_name.clone();
    let conf = msg.confidence;
    let duration = msg.duration_ms;
    let intent = msg.intent.clone();
    let artifact_id = msg.artifact_id.as_ref().map(|id| id.as_string());
    let tool_trace = msg.tool_trace.clone();
    let skill_trace = msg.skill_trace.clone();
    let message_id = msg.id.to_string();
    let content = msg.content.clone();
    let attachment_chat_id = msg.chat_id.as_string();
    let attachments = msg.attachments.clone();
    let time = format_utc_local(&msg.created_at, "%d.%m %H:%M:%S");
    let (avatar, author) = if is_user {
        ("avatar-user", "ВЫ")
    } else {
        ("avatar-assistant", "АССИСТЕНТ")
    };
    // Нейтральные оттенки серого вместо синеватого фона: пользователь — чуть
    // темнее (Background3), ассистент — базовый фон (Background1).
    let row_style = if is_user {
        "width: 100%; padding: 12px 16px; background: var(--colorNeutralBackground3);"
    } else {
        "width: 100%; padding: 12px 16px; background: var(--colorNeutralBackground1);"
    };
    view! {
        <div style=row_style>
            <div style="max-width: 980px; margin: 0 auto; display: flex; gap: 16px;">
                {FeedGutter(avatar, author, time)}
                <div style="flex: 1; min-width: 0;">
                    {if content.trim().is_empty() {
                        ().into_any()
                    } else if is_user {
                        view! { <KbLinkedText text=content /> }.into_any()
                    } else {
                        view! { <Markdown text=content /> }.into_any()
                    }}
                    {(!attachments.is_empty()).then(|| view! {
                        <div style="display: flex; flex-wrap: wrap; gap: 8px; margin-top: 8px;">
                            {attachments.into_iter().map(|attachment| {
                                if attachment.content_type.starts_with("image/") {
                                    view! {
                                        <AttachmentImage
                                            chat_id=attachment_chat_id.clone()
                                            attachment=attachment
                                        />
                                    }.into_any()
                                } else {
                                    view! {
                                        <span style="display:inline-flex;align-items:center;gap:5px;padding:5px 8px;border:1px solid var(--colorNeutralStroke2);border-radius:6px;">
                                            {icon("document")} {attachment.filename}
                                        </span>
                                    }.into_any()
                                }
                            }).collect_view()}
                        </div>
                    })}
                    {move || {
                        if !prefs.get().show_skill_warnings {
                            return None;
                        }
                        let inefficient: Vec<String> = skill_trace
                            .as_deref()
                            .and_then(|raw| {
                                serde_json::from_str::<Vec<
                                    contracts::domain::a018_llm_chat::aggregate::SkillTraceEntry,
                                >>(raw)
                                .ok()
                            })
                            .unwrap_or_default()
                            .into_iter()
                            .filter(|item| item.inefficient)
                            .map(|item| item.skill_id)
                            .collect();
                        (!inefficient.is_empty()).then(|| view! {
                            <div style="margin-top:8px;padding:7px 10px;border:1px solid #fcd34d;border-radius:6px;background:#fffbeb;color:#92400e;font-size:12px;">
                                {format!(
                                    "Использован расширенный навык {} — задача выходит за основную специализацию сотрудника.",
                                    inefficient.join(", ")
                                )}
                            </div>
                        })
                    }}
                    {move || {
                        if !prefs.get().show_meta_line {
                            return None;
                        }
                        let show_tokens = prefs.get().show_tokens;
                        let mut meta_parts = Vec::new();
                        if let Some(i) = &intent {
                            let label = match i.as_str() {
                                "func_help" => "🧭 функционал",
                                "data_query" => "📊 данные",
                                "bi_authoring" => "📈 BI-сборка",
                                "plugin_dev" => "🧩 плагин",
                                "sys_admin" => "🛠 система",
                                "kb_curation" => "📚 база знаний",
                                "meta_smalltalk" => "💬 диалог",
                                other => other,
                            };
                            meta_parts.push(label.to_string());
                        }
                        if let (true, Some(t)) = (show_tokens, tokens) {
                            meta_parts.push(format!("🎫 {} tokens", t));
                        }
                        if let Some(m) = &model {
                            meta_parts.push(format!("🤖 {}", m));
                        }
                        if let Some(d) = duration {
                            meta_parts.push(format!("⏱ {:.1}s", d as f64 / 1000.0));
                        }
                        if let Some(c) = conf {
                            meta_parts.push(format!("📊 {:.1}%", c * 100.0));
                        }
                        if !meta_parts.is_empty() {
                            Some(
                                view! {
                                    <div style="font-size: 11px; opacity: 0.7; margin-top: 6px;">
                                        {meta_parts.join(" • ")}
                                    </div>
                                },
                            )
                        } else {
                            None
                        }
                    }}

                    {move || {
                        if !is_user && prefs.get().show_tool_calls {
                            Some(view! { <ToolCallsTrace tool_trace=tool_trace.clone() message_id=message_id.clone() /> })
                        } else {
                            None
                        }
                    }}
                    {move || {
                        artifact_id
                            .clone()
                            .map(|id| view! { <ArtifactCard artifact_id=id /> })
                    }}
                </div>
            </div>
        </div>
    }
}

/// Строка-событие: к чату прикреплён пакет контекста страницы. Ссылка ведёт на
/// details-страницу контекста (тот же дизайн, что был у чипа контекста).
#[allow(non_snake_case)]
fn ContextRow(p: ContextPackageSummary, nav_ctx: Option<AppGlobalContext>) -> impl IntoView {
    let time = format_datetime_utc_local(&p.created_at, "%d.%m %H:%M:%S");
    let tab_key = format!("a018_llm_context_details_{}", p.id);
    let title = p.title.clone();
    let link_title = p.title.clone();
    let page_key = p.page_key.clone();
    view! {
        <div style="width: 100%; padding: 10px 16px; background: var(--colorNeutralBackground2);">
            <div style="max-width: 980px; margin: 0 auto; display: flex; gap: 16px;">
                {FeedGutter("avatar-context", "КОНТЕКСТ", time)}
                <div style="flex: 1; min-width: 0; display: flex; align-items: center; flex-wrap: wrap; gap: 6px;">
                    <span style="opacity: 0.7; font-size: 13px;">"Добавлен документ в контекст:"</span>
                    <a
                        href="#"
                        title=page_key
                        style="display: inline-flex; align-items: center; gap: 6px; \
                               color: var(--colorBrandForeground1); text-decoration: none; \
                               cursor: pointer; font-size: 13px;"
                        on:click=move |e| {
                            e.prevent_default();
                            if let Some(c) = nav_ctx {
                                c.open_tab(&tab_key, &format!("Контекст: {}", title));
                            }
                        }
                    >
                        {icon("paperclip")}
                        {format!(" {}", link_title)}
                    </a>
                </div>
            </div>
        </div>
    }
}

#[component]
#[allow(non_snake_case)]
pub fn LlmChatDetails(id: String, on_close: Callback<()>) -> impl IntoView {
    let vm = LlmChatDetailsVm::new();
    let chat_id = id.clone();
    let messages_container_ref = NodeRef::<leptos::html::Div>::new();
    let context_pkgs = RwSignal::new(Vec::<
        contracts::domain::a018_llm_chat::context::ContextPackageSummary,
    >::new());
    // Сколько секунд выполняется текущий запрос к LLM (тикает раз в секунду,
    // пока vm.is_sending == true). Показывается под индикатором набора.
    let elapsed_secs = RwSignal::new(0u32);
    // Текущий этап выполнения LLM-задачи (с бэкенда через polling). None — пока
    // не известен; тогда показываем дефолтную подпись.
    let progress = RwSignal::new(None::<JobProgress>);
    // job_id текущей фоновой задачи — для кнопки «Стоп» (cancel).
    let current_job_id = RwSignal::new(None::<String>);
    // Панель «Диагностика»: открыта ли и текст опционального комментария пользователя.
    let diag_open = RwSignal::new(false);
    let diag_comment = RwSignal::new(String::new());
    let kb_open = RwSignal::new(false);
    let kb_comment = RwSignal::new(String::new());
    // Рабочий каталог задачи: состояние поднято сюда, потому что им пользуются
    // сразу двое — drawer «Файлы задачи» и бар уточняющих вопросов над вводом.
    let workspace = RwSignal::new(ChatWorkspaceView::default());
    let workspace_drawer_open = RwSignal::new(false);
    let settings_open = RwSignal::new(false);
    // Что показывать в ленте — общая для всех чатов настройка из localStorage.
    let ui_prefs = RwSignal::new(ChatUiPrefs::load());
    let chat_id_stored = StoredValue::new(chat_id.clone());
    let reload_workspace = Callback::new(move |_| {
        let id = chat_id_stored.get_value();
        wasm_bindgen_futures::spawn_local(async move {
            if let Ok(view) = fetch_workspace(&id).await {
                workspace.set(view);
            }
        });
    });
    // Переключатель модели в чате: allowed_models — курируемый список моделей подключения,
    // selected_model — текущий выбор (прокидывается на каждое сообщение).
    let allowed_models = RwSignal::new(Vec::<String>::new());
    let image_input_models = RwSignal::new(Vec::<String>::new());
    let selected_model = RwSignal::new(String::new());
    let model_capabilities_loaded = RwSignal::new(false);
    let model_is_saving = RwSignal::new(false);
    let pending_screenshot = RwSignal::new_local(None::<PendingScreenshot>);
    let nav_ctx = use_context::<AppGlobalContext>();

    // Единая точка загрузки вложения: и файл-пикер, и подтверждённый скриншот
    // проходят через неё (upload → показ чипа с превью → откат object-URL при ошибке).
    let attach_file = {
        let chat_id = chat_id.clone();
        UnsyncCallback::new(
            move |(file, preview_url): (web_sys::File, Option<String>)| {
                let chat_id = chat_id.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    match super::api::upload_file(&chat_id, file).await {
                        Ok(mut file_info) => {
                            file_info.preview_url = preview_url;
                            vm.uploaded_files.update(|files| files.push(file_info));
                        }
                        Err(e) => {
                            if let Some(url) = &preview_url {
                                let _ = web_sys::Url::revoke_object_url(url);
                            }
                            vm.error.set(Some(format!("Ошибка загрузки файла: {}", e)));
                        }
                    }
                });
            },
        )
    };

    // Открыть редактор для только что вставленного/выбранного изображения.
    let begin_screenshot_edit = UnsyncCallback::new(move |file: web_sys::File| {
        if let Some(previous) = pending_screenshot.get_untracked() {
            previous.revoke();
        }
        match PendingScreenshot::open(file) {
            Ok(pending) => pending_screenshot.set(Some(pending)),
            Err(()) => vm.error.set(Some(
                "Не удалось открыть скриншот из буфера обмена.".to_string(),
            )),
        }
    });

    // Ревок черновых object-URL при уходе со страницы (иначе утечка до перезагрузки).
    on_cleanup(move || {
        for file in vm.uploaded_files.get_untracked() {
            if let Some(url) = file.preview_url {
                let _ = web_sys::Url::revoke_object_url(&url);
            }
        }
        if let Some(pending) = pending_screenshot.get_untracked() {
            pending.revoke();
        }
    });

    // Scroll to bottom helper
    let scroll_to_bottom = {
        let messages_container_ref = messages_container_ref.clone();
        move || {
            if let Some(container) = messages_container_ref.get() {
                request_animation_frame(move || {
                    container.set_scroll_top(container.scroll_height());
                });
            }
        }
    };

    // Send message handler - using Callback to avoid move issues
    let handle_send = Callback::new({
        let chat_id = chat_id.clone();
        let scroll_to_bottom = scroll_to_bottom.clone();
        move |_| {
            let content = vm.new_message.get();
            let draft_files = vm.uploaded_files.get();
            if content.trim().is_empty() && draft_files.is_empty() {
                return;
            }
            let current_model = selected_model.get_untracked();
            let current_allowed_models = allowed_models.get_untracked();
            if model_capabilities_loaded.get_untracked()
                && !current_allowed_models.is_empty()
                && !current_allowed_models
                    .iter()
                    .any(|model| model == &current_model)
            {
                vm.error.set(Some(format!(
                    "Модель '{}' недоступна сотруднику. Выберите в заголовке одну из доступных моделей: {}.",
                    current_model,
                    current_allowed_models.join(", ")
                )));
                return;
            }
            if model_is_saving.get_untracked() {
                vm.error.set(Some(
                    "Подождите, пока выбранная модель сохранится в чате.".to_string(),
                ));
                return;
            }
            if draft_files.iter().any(|file| file.is_image())
                && !image_input_models
                    .get_untracked()
                    .iter()
                    .any(|model| model == &selected_model.get_untracked())
            {
                vm.error.set(Some(
                    "Выбранная модель не поддерживает изображения. Скриншот сохранён в сообщении — переключите модель в заголовке чата."
                        .to_string(),
                ));
                return;
            }

            vm.is_sending.set(true);
            vm.new_message.set(String::new());

            // Запустить секундный таймер на время ожидания ответа LLM.
            elapsed_secs.set(0);
            progress.set(None);
            {
                let start = js_sys::Date::now();
                let is_sending = vm.is_sending;
                wasm_bindgen_futures::spawn_local(async move {
                    while is_sending.get_untracked() {
                        gloo_timers::future::TimeoutFuture::new(1000).await;
                        if !is_sending.get_untracked() {
                            break;
                        }
                        elapsed_secs.set(((js_sys::Date::now() - start) / 1000.0) as u32);
                    }
                });
            }

            // Create optimistic user message
            let chat_uuid = Uuid::parse_str(&chat_id).unwrap_or_else(|_| Uuid::new_v4());
            let chat_id_obj =
                contracts::domain::a018_llm_chat::aggregate::LlmChatId::new(chat_uuid);
            let mut optimistic_msg = LlmChatMessage::user(chat_id_obj, content.clone());
            optimistic_msg.attachments = draft_files
                .iter()
                .filter_map(|file| {
                    Some(LlmChatAttachmentSummary {
                        id: Uuid::parse_str(&file.id).ok()?,
                        filename: file.filename.clone(),
                        content_type: file.content_type.clone(),
                        file_size: file.file_size,
                    })
                })
                .collect();
            let optimistic_id = optimistic_msg.id;

            // Add optimistic message immediately
            let mut current_msgs = vm.messages.get();
            current_msgs.push(optimistic_msg);
            vm.messages.set(current_msgs);
            scroll_to_bottom();

            let chat_id = chat_id.clone();
            let scroll_to_bottom = scroll_to_bottom.clone();
            let attachment_ids = draft_files.iter().map(|f| f.id.clone()).collect();
            wasm_bindgen_futures::spawn_local(async move {
                // 1. POST → immediately get job_id (server returns 202)
                let model_choice = Some(selected_model.get_untracked());
                let job_id =
                    match send_message(&chat_id, &content, attachment_ids, model_choice).await {
                        Ok(id) => id,
                        Err(e) => {
                            vm.error.set(Some(format!("Ошибка отправки: {}", e)));
                            vm.new_message.set(content.clone());
                            let mut current_msgs = vm.messages.get();
                            current_msgs.retain(|msg| msg.id != optimistic_id);
                            vm.messages.set(current_msgs);
                            vm.is_sending.set(false);
                            return;
                        }
                    };
                current_job_id.set(Some(job_id.clone()));

                // 2. Опрос статуса каждые 500мс: progress.partial_text несёт частичный
                //    текст ответа (стриминг), поэтому частый опрос = живой вывод.
                //    Бюджет — 6 минут: агентные навыки делают много шагов tool-calling.
                let poll_result = poll_until_done(&job_id, 720, 500, progress).await;
                current_job_id.set(None);

                // 3. Always reload messages from DB after completion
                match fetch_messages(&chat_id).await {
                    Ok(msgs) => {
                        vm.messages.set(msgs);
                        scroll_to_bottom();
                    }
                    Err(_) => {
                        let mut current_msgs = vm.messages.get();
                        current_msgs.retain(|msg| msg.id != optimistic_id);
                        vm.messages.set(current_msgs);
                    }
                }

                // Перезагрузить пакеты контекста: документы, прикреплённые во время
                // сессии, должны появиться в ленте на своих местах по времени.
                if let Ok(pkgs) = fetch_chat_context(&chat_id).await {
                    context_pkgs.set(pkgs);
                }

                // Ход ассистента мог добавить уточняющие вопросы — бар над вводом
                // должен показать их сразу, без открытия каталога.
                reload_workspace.run(());

                match poll_result {
                    Ok(PollOutcome::Done) => {
                        for file in vm.uploaded_files.get_untracked() {
                            if let Some(url) = file.preview_url {
                                let _ = web_sys::Url::revoke_object_url(&url);
                            }
                        }
                        vm.uploaded_files.set(Vec::new());
                        vm.error.set(None);
                    }
                    Ok(PollOutcome::StillRunning { waited_secs }) => {
                        // Задача не уложилась в бюджет ожидания, но продолжает выполняться
                        // на сервере и допишет ответ сама. Это не ошибка — мягко поясняем.
                        vm.error.set(Some(format!(
                            "Ответ готовится дольше обычного (прошло ~{} мин, сложная задача \
                             с несколькими шагами). Он появится в чате автоматически — \
                             обновите страницу через минуту, если не появился.",
                            waited_secs.max(60) / 60
                        )));
                    }
                    Ok(PollOutcome::Error(msg)) if msg == "cancelled" => {
                        // Пользователь нажал «Стоп» — это не ошибка.
                        vm.error.set(Some("Генерация остановлена.".to_string()));
                    }
                    Ok(PollOutcome::Error(msg)) => {
                        vm.new_message.set(content.clone());
                        vm.error.set(Some(format!("Ошибка LLM: {}", msg)));
                    }
                    Err(e) => {
                        vm.error
                            .set(Some(format!("Ошибка связи при ожидании ответа: {}", e)));
                    }
                }

                vm.is_sending.set(false);
                progress.set(None);
            });
        }
    });

    // Load chat and messages; затем, если чат только что создан со страницы списка,
    // автоматически отправить первый вопрос пользователя (handle_send покажет
    // оптимистичное сообщение, индикатор набора и подгрузит ответ).
    Effect::new({
        let chat_id = chat_id.clone();
        let scroll_to_bottom = scroll_to_bottom.clone();
        let ctx = use_context::<AppGlobalContext>();
        move |_| {
            let chat_id = chat_id.clone();
            let scroll_to_bottom = scroll_to_bottom.clone();
            wasm_bindgen_futures::spawn_local(async move {
                // Load chat
                match fetch_chat(&chat_id).await {
                    Ok(chat) => {
                        let conn_id = chat.chat.agent_id.as_string();
                        if selected_model.get_untracked().is_empty() {
                            selected_model.set(chat.chat.model_name.clone());
                        }
                        vm.chat.set(Some(chat));
                        // Курируемый список моделей подключения для переключателя.
                        if let Ok((allowed, image_models)) =
                            fetch_connection_model_capabilities(&conn_id).await
                        {
                            allowed_models.set(allowed);
                            image_input_models.set(image_models);
                            model_capabilities_loaded.set(true);
                        }
                    }
                    Err(e) => vm.error.set(Some(e)),
                }

                // Load messages
                match fetch_messages(&chat_id).await {
                    Ok(msgs) => {
                        vm.messages.set(msgs);
                        vm.error.set(None);
                        scroll_to_bottom();
                    }
                    Err(e) => vm.error.set(Some(e)),
                }

                // Load attached page-context packages (for the context chip strip).
                if let Ok(pkgs) = fetch_chat_context(&chat_id).await {
                    context_pkgs.set(pkgs);
                }

                // Каталог задачи читаем сразу: неотвеченные вопросы модели должны
                // быть видны над полем ввода при первом же открытии чата.
                reload_workspace.run(());

                // Авто-отправка первого вопроса для только что созданного чата.
                if let Some(ctx) = ctx {
                    let key = pending_first_message_key(&chat_id);
                    let pending = ctx
                        .get_form_state(&key)
                        .and_then(|v| v.as_str().map(|s| s.to_string()));
                    if let Some(pending) = pending {
                        // Одноразово: очистить, чтобы не переотправлять при ремоунте вкладки.
                        ctx.set_form_state(key, serde_json::Value::Null);
                        if !pending.trim().is_empty() {
                            vm.new_message.set(pending);
                            handle_send.run(());
                        }
                    }
                }
            });
        }
    });

    // Реактивно перезагружать ленту контекста, когда документ добавлен из шапки
    // (`AiChatHeaderButton`) к уже открытому чату: вкладка не переоткрывается, а
    // версия контекста в `form_states` инкрементируется — на это и реагируем.
    Effect::new({
        let chat_id = chat_id.clone();
        let ctx = use_context::<AppGlobalContext>();
        let last_seen = RwSignal::new(None::<u64>);
        move |_| {
            let Some(ctx) = ctx else { return };
            let key = crate::domain::a018_llm_chat::ui::context_version_key(&chat_id);
            // Tracked-чтение карты: эффект перевызывается при любом изменении.
            let version = ctx
                .form_states
                .with(|m| m.get(&key).and_then(|v| v.as_u64()).unwrap_or(0));
            match last_seen.get_untracked() {
                None => {
                    // Первый прогон: запомнить текущую версию, не перезагружая.
                    last_seen.set(Some(version));
                    return;
                }
                Some(prev) if prev == version => return,
                _ => {}
            }
            last_seen.set(Some(version));

            let chat_id = chat_id.clone();
            wasm_bindgen_futures::spawn_local(async move {
                if let Ok(pkgs) = fetch_chat_context(&chat_id).await {
                    context_pkgs.set(pkgs);
                }
            });
        }
    });

    // Клон chat_id для виджета оценки (остальные клоны разошлись по замыканиям выше).
    let chat_id_for_rating = chat_id.clone();
    let chat_id_for_delete = chat_id.clone();
    let chat_id_for_draft_attachments = chat_id.clone();
    let cancel_screenshot = UnsyncCallback::new(move |_| {
        if let Some(pending) = pending_screenshot.get_untracked() {
            pending.revoke();
        }
        pending_screenshot.set(None);
    });
    // Ответ на уточняющий вопрос: он уже записан в анкету, остаётся проговорить
    // его в чате, чтобы модель продолжила ход. Черновик композера возвращаем на
    // место — `handle_send` читает и чистит `new_message` синхронно.
    // Оговорка: если отправка упадёт, error-путь `handle_send` вернёт в композер
    // текст ответа поверх черновика — редкий и безобидный случай.
    let answer_question = Callback::new(move |msg: String| {
        let draft = vm.new_message.get_untracked();
        vm.new_message.set(msg);
        handle_send.run(());
        vm.new_message.set(draft);
        reload_workspace.run(());
    });

    // Подтверждение: редактор отдаёт уже собранный (возможно, с аннотациями) файл.
    // Модалку закрываем сразу, загрузку ведём в фоне через общий attach_file;
    // исходный object-URL ревокаем, для чипа делаем новый — из итогового файла.
    let confirm_screenshot = UnsyncCallback::new(move |edited: web_sys::File| {
        let Some(pending) = pending_screenshot.get_untracked() else {
            return;
        };
        pending.revoke();
        pending_screenshot.set(None);
        let preview_url = web_sys::Url::create_object_url_with_blob(&edited).ok();
        attach_file.run((edited, preview_url));
    });

    view! {
        <PageFrame page_id="a018_llm_chat--detail" category=PAGE_CAT_DETAIL class="a018-llm-chat-detail">
            <div class="page__header" style="flex-wrap: wrap; height: auto; gap: 8px 12px;">
                <div class="page__header-left" style="flex-wrap: wrap;">
                    <h1 class="page__title" style="white-space: normal; overflow-wrap: anywhere;">
                        {move || {
                            vm.chat
                                .get()
                                .map(|c| c.chat.base.description.clone())
                                .unwrap_or_else(|| "Загрузка...".to_string())
                        }}
                    </h1>
                    <span class="page__header-meta">
                        {move || {
                            vm.chat.get().map(|c| {
                                let agent_display = c.agent_name.clone().unwrap_or_else(|| c.chat.agent_id.as_string());
                                format!("Агент: {}", agent_display)
                            }).unwrap_or_default()
                        }}
                    </span>
                    <span class="page__header-meta">
                        "Модель: "
                        <select
                            style=move || {
                                let current = selected_model.get();
                                let allowed = allowed_models.get();
                                let invalid = model_capabilities_loaded.get()
                                    && !allowed.is_empty()
                                    && !allowed.contains(&current);
                                let border = if invalid { "#d13438" } else { "var(--colorNeutralStroke2)" };
                                format!("height: 24px; padding: 0 4px; border: 2px solid {}; border-radius: 4px; background: var(--color-surface); color: var(--color-text);", border)
                            }
                            prop:value=move || selected_model.get()
                            on:change=move |ev| {
                                let next = event_target_value(&ev);
                                let previous = selected_model.get_untracked();
                                if next == previous {
                                    return;
                                }
                                selected_model.set(next.clone());
                                model_is_saving.set(true);
                                let chat_id = chat_id_stored.get_value();
                                wasm_bindgen_futures::spawn_local(async move {
                                    match set_chat_model(&chat_id, &next).await {
                                        Ok(()) => {
                                            vm.chat.update(|detail| {
                                                if let Some(detail) = detail {
                                                    detail.chat.model_name = next;
                                                }
                                            });
                                            vm.error.set(None);
                                        }
                                        Err(error) => {
                                            selected_model.set(previous);
                                            vm.error.set(Some(format!(
                                                "Не удалось изменить модель чата: {}",
                                                error
                                            )));
                                        }
                                    }
                                    model_is_saving.set(false);
                                });
                            }
                            title="Модель ограничена списком allowed_models подключения"
                        >
                            {move || {
                                let mut list = allowed_models.get();
                                let current = selected_model.get();
                                if !current.is_empty() && !list.contains(&current) {
                                    list.insert(0, current);
                                }
                                if list.is_empty() {
                                    let m = selected_model.get();
                                    if !m.is_empty() {
                                        list = vec![m];
                                    }
                                }
                                list.into_iter()
                                    .map(|m| {
                                        let label = if model_capabilities_loaded.get()
                                            && !allowed_models.get().is_empty()
                                            && !allowed_models.get().contains(&m)
                                        {
                                            format!("{} (недоступна)", m)
                                        } else {
                                            m.clone()
                                        };
                                        view! { <option value=m>{label}</option> }
                                    })
                                    .collect_view()
                            }}
                        </select>
                    </span>
                    {move || {
                        let current = selected_model.get();
                        let allowed = allowed_models.get();
                        let invalid = model_capabilities_loaded.get()
                            && !allowed.is_empty()
                            && !allowed.contains(&current);
                        invalid.then(|| view! {
                            <span style="color: #d13438; font-size: 12px;">
                                {format!(
                                    "Модель недоступна сотруднику. Выберите: {}",
                                    allowed.join(", ")
                                )}
                            </span>
                        })
                    }}
                    <span class="page__header-meta">
                        {move || format!("Сообщений: {}", vm.messages.get().len())}
                    </span>
                </div>
                <div class="page__header-right" style="display: flex; align-items: center; gap: 12px;">
                    // Оценка чата: 5 звёзд. Клик по текущей звезде снимает оценку.
                    // Звёзды идут перед кнопками.
                    <div
                        title="Оценить чат"
                        style="display: inline-flex; gap: 2px; font-size: 20px; line-height: 1;"
                    >
                        {move || {
                            let cid = chat_id_for_rating.clone();
                            let current = vm.chat.get().and_then(|c| c.chat.rating).unwrap_or(0);
                            (1..=5)
                                .map(|n| {
                                    let cid = cid.clone();
                                    let filled = n <= current;
                                    view! {
                                        <button
                                            type="button"
                                            title=move || format!("Оценка: {}", n)
                                            style=move || format!(
                                                "background:none;border:none;cursor:pointer;padding:0 1px;line-height:1;color:{};",
                                                if filled { "#f5a623" } else { "var(--color-text-secondary, #9ca3af)" }
                                            )
                                            on:click=move |_| {
                                                let cid = cid.clone();
                                                let target = if current == n { None } else { Some(n) };
                                                wasm_bindgen_futures::spawn_local(async move {
                                                    match set_rating(&cid, target).await {
                                                        Ok(()) => vm.chat.update(|opt| {
                                                            if let Some(c) = opt { c.chat.rating = target; }
                                                        }),
                                                        Err(e) => vm.error.set(Some(format!("Ошибка оценки: {}", e))),
                                                    }
                                                });
                                            }
                                        >
                                            {if filled { "★" } else { "☆" }}
                                        </button>
                                    }
                                })
                                .collect_view()
                        }}
                    </div>
                    // Технические действия убраны под «Ещё»: в шапке чата важнее
                    // сам разговор, а не ряд редко нужных кнопок.
                    <div style="display: flex; align-items: center; gap: 8px;">
                        <MoreActionsMenu>
                            // Диагностика: открывает модальный диалог с комментарием;
                            // запуск проверки закрывает диалог и отправляет промпт в чат.
                            <button
                                class="theme-dropdown__item"
                                on:click=move |_| {
                                    use_more_actions_close();
                                    if vm.is_sending.get_untracked() {
                                        return;
                                    }
                                    diag_open.set(true);
                                }
                            >
                                <span style="display: flex; align-items: center; gap: 8px;">
                                    {icon("search")} "Диагностика"
                                </span>
                            </button>
                            // В базу знаний: выжать из диалога статью. Тот же приём,
                            // что и у диагностики — уточнение, затем промпт в чат.
                            <button
                                class="theme-dropdown__item"
                                on:click=move |_| {
                                    use_more_actions_close();
                                    if vm.is_sending.get_untracked() {
                                        return;
                                    }
                                    kb_open.set(true);
                                }
                            >
                                <span style="display: flex; align-items: center; gap: 8px;">
                                    {icon("book-open-text")} "В базу знаний"
                                </span>
                            </button>
                            <button
                                class="theme-dropdown__item"
                                on:click=move |_| {
                                    use_more_actions_close();
                                    workspace_drawer_open.set(true);
                                }
                            >
                                <span style="display: flex; align-items: center; gap: 8px;">
                                    {icon("folder")} "Файлы задачи"
                                </span>
                            </button>
                            <button
                                class="theme-dropdown__item"
                                on:click=move |_| {
                                    use_more_actions_close();
                                    settings_open.set(true);
                                }
                            >
                                <span style="display: flex; align-items: center; gap: 8px;">
                                    {icon("settings")} "Настройки чата"
                                </span>
                            </button>
                            <div class="chat-more__separator"></div>
                            <button
                                class="theme-dropdown__item"
                                on:click=move |_| {
                                    use_more_actions_close();
                                    let confirmed = web_sys::window()
                                        .and_then(|win| win.confirm_with_message("Удалить чат?").ok())
                                        .unwrap_or(false);
                                    if !confirmed {
                                        return;
                                    }
                                    let id = chat_id_for_delete.clone();
                                    wasm_bindgen_futures::spawn_local(async move {
                                        match delete_chat(&id).await {
                                            Ok(()) => on_close.run(()),
                                            Err(e) => vm.error.set(Some(format!("Ошибка удаления: {}", e))),
                                        }
                                    });
                                }
                            >
                                <span style="display: flex; align-items: center; gap: 8px;">
                                    {icon("delete")} "Удалить"
                                </span>
                            </button>
                        </MoreActionsMenu>
                        <Button
                            appearance=ButtonAppearance::Secondary
                            on_click=move |_| on_close.run(())
                        >
                            {icon("x")}
                            " Закрыть"
                        </Button>
                    </div>
                </div>
            </div>

            <div
                class="page__content"
                style="display: flex; flex-direction: column; min-height: 0;"
                on:paste=move |ev: web_sys::ClipboardEvent| {
                    let Some(clipboard) = ev.clipboard_data() else {
                        return;
                    };
                    let Some(files) = clipboard.files() else {
                        return;
                    };
                    for index in 0..files.length() {
                        let Some(file) = files.get(index) else {
                            continue;
                        };
                        if !is_editable_image_type(&file.type_()) {
                            continue;
                        }
                        ev.prevent_default();
                        begin_screenshot_edit.run(file);
                        break;
                    }
                }
            >
                // Error display
                {move || {
                    vm.error
                        .get()
                        .map(|e| {
                            view! {
                                <div class="warning-box" style="background: var(--color-error-50); border-color: var(--color-error-100); margin-bottom: var(--spacing-md);">
                                    <span class="warning-box__text" style="color: var(--color-error);">{e}</span>
                                </div>
                            }
                        })
                }}

                // Messages area — full width, no frame; rows distinguished by background.
                // Сообщения и события прикрепления контекста показываются единой
                // лентой в хронологическом порядке (сортировка по времени создания).
                <div
                node_ref=messages_container_ref
                style="flex: 1; overflow-y: auto; display: flex; flex-direction: column; gap: 2px; margin-bottom: 16px;"
            >
                <For
                    each=move || {
                        let mut rows: Vec<FeedRow> = Vec::new();
                        for m in vm.messages.get() {
                            rows.push(FeedRow {
                                ts: m.created_at,
                                key: format!("m-{}", m.id),
                                item: FeedItem::Message(m),
                            });
                        }
                        // Прикрепление документов — опциональный слой ленты: keyed diff
                        // по `c-*` уберёт/вернёт только их, сообщения не пересоздаются.
                        for p in if ui_prefs.get().show_context_events {
                            context_pkgs.get()
                        } else {
                            Vec::new()
                        } {
                            let ts = chrono::DateTime::parse_from_rfc3339(&p.created_at)
                                .map(|d| d.with_timezone(&chrono::Utc))
                                .unwrap_or_else(|_| chrono::Utc::now());
                            rows.push(FeedRow {
                                ts,
                                key: format!("c-{}", p.id),
                                item: FeedItem::Context(p),
                            });
                        }
                        rows.sort_by(|a, b| a.ts.cmp(&b.ts));
                        rows
                    }
                    key=|row| row.key.clone()
                    let:row
                >
                    {match row.item {
                        FeedItem::Message(msg) => MessageRow(msg, ui_prefs).into_any(),
                        FeedItem::Context(p) => ContextRow(p, nav_ctx).into_any(),
                    }}
                </For>

                // Loading indicator — показывается пока LLM обрабатывает запрос.
                // Если стриминг уже принёс частичный текст ответа — рендерим его
                // (живой вывод, как в Claude/ChatGPT), под ним — этап и кнопка «Стоп».
                {move || {
                    if vm.is_sending.get() {
                        Some(view! {
                            <div style="display: flex; flex-direction: column; gap: 6px; align-self: stretch;">
                                {move || {
                                    progress.get()
                                        .and_then(|p| p.partial_text)
                                        .filter(|t| !t.trim().is_empty())
                                        .map(|partial| view! {
                                            <div style="display: flex; gap: 12px; padding: 10px 14px; border-radius: 8px; background: var(--colorNeutralBackground1); opacity: 0.9;">
                                                <div style="flex: 0 0 104px; font-size: 11px; font-weight: 600; opacity: 0.6;">
                                                    "АССИСТЕНТ"
                                                </div>
                                                <div style="flex: 1; min-width: 0;">
                                                    <Markdown text=partial />
                                                </div>
                                            </div>
                                        })
                                }}
                                <div class="chat-typing" style="align-self: flex-start; max-width: 70%; display: flex; align-items: center; gap: 10px;">
                                    <div class="chat-typing__bubble">
                                        <span class="chat-typing__dot"></span>
                                        <span class="chat-typing__dot"></span>
                                        <span class="chat-typing__dot"></span>
                                        <span class="chat-typing__label">
                                            {move || {
                                                let secs = elapsed_secs.get();
                                                match progress.get() {
                                                    Some(p) if p.step > 0 => {
                                                        format!(" Шаг {} · {} · {} с", p.step, p.stage, secs)
                                                    }
                                                    Some(p) => format!(" {} · {} с", p.stage, secs),
                                                    None => format!(" LLM обрабатывает запрос… {} с", secs),
                                                }
                                            }}
                                        </span>
                                    </div>
                                    {move || {
                                        current_job_id.get().map(|job_id| view! {
                                            <button
                                                title="Остановить генерацию"
                                                style="border: 1px solid var(--colorPaletteRedBorder2, #b10e1c); background: var(--colorPaletteRedBackground3, #c50f1f); color: #fff; border-radius: 6px; padding: 4px 10px; font-size: 12px; font-weight: 600; cursor: pointer;"
                                                on:click=move |_| {
                                                    let job_id = job_id.clone();
                                                    wasm_bindgen_futures::spawn_local(async move {
                                                        let _ = cancel_job(&job_id).await;
                                                    });
                                                }
                                            >
                                                "■ Стоп"
                                            </button>
                                        })
                                    }}
                                </div>
                            </div>
                        })
                    } else {
                        None
                    }
                }}
                </div>

                // Input area — фиксированная ширина по колонке ленты, по центру,
                // чтобы поле ввода не растягивалось на весь экран.
                <div style="display: flex; flex-direction: column; gap: 8px; max-width: 980px; width: 100%; margin: 0 auto;">
                // Уточняющие вопросы модели — над вводом: там, где человек и так
                // собирается отвечать. Ответ фиксируется в анкете и уходит в чат.
                <ChatQuestionsBar
                    chat_id=chat_id_stored
                    workspace=workspace
                    is_sending=vm.is_sending
                    on_answered=answer_question
                />
                // File attachments display
                {move || {
                    let chat_id = chat_id_for_draft_attachments.clone();
                    let files = vm.uploaded_files.get();
                    if !files.is_empty() {
                        Some(
                            view! {
                                <Flex style="gap: 8px; flex-wrap: wrap;">
                                    <For
                                        each=move || vm.uploaded_files.get()
                                        key=|f| f.id.clone()
                                        let:file
                                    >
                                        <div style="padding: 6px; background: var(--colorNeutralBackground2); border: 1px solid var(--colorNeutralStroke2); border-radius: 6px; display: flex; align-items: center; gap: 8px;">
                                            {if file.is_image() {
                                                let preview_url = file.preview_url.clone().unwrap_or_default();
                                                let open_url = preview_url.clone();
                                                view! {
                                                    <button
                                                        type="button"
                                                        title="Открыть просмотр"
                                                        style="border:0;background:none;padding:0;cursor:pointer;line-height:0;"
                                                        on:click=move |_| {
                                                            if let Some(window) = web_sys::window() {
                                                                let _ = window.open_with_url_and_target(&open_url, "_blank");
                                                            }
                                                        }
                                                    >
                                                        <img
                                                            src=preview_url
                                                            alt=file.filename.clone()
                                                            style="display:block;width:96px;height:64px;object-fit:contain;border-radius:4px;"
                                                        />
                                                    </button>
                                                }.into_any()
                                            } else {
                                                view! {
                                                    <span style="font-size: 14px;">
                                                        {icon("document")} " " {file.filename.clone()}
                                                    </span>
                                                }.into_any()
                                            }}
                                            <button
                                                title="Удалить вложение"
                                                style="background: none; border: none; cursor: pointer; padding: 2px; color: var(--colorNeutralForeground3);"
                                                on:click={
                                                    let file_id = file.id.clone();
                                                    let preview_url = file.preview_url.clone();
                                                    let chat_id = chat_id.clone();
                                                    move |_| {
                                                        let mut files = vm.uploaded_files.get();
                                                        files.retain(|f| f.id != file_id);
                                                        vm.uploaded_files.set(files);
                                                        if let Some(url) = &preview_url {
                                                            let _ = web_sys::Url::revoke_object_url(url);
                                                        }
                                                        let chat_id = chat_id.clone();
                                                        let file_id = file_id.clone();
                                                        wasm_bindgen_futures::spawn_local(async move {
                                                            if let Err(error) =
                                                                delete_pending_attachment(&chat_id, &file_id).await
                                                            {
                                                                vm.error.set(Some(format!(
                                                                    "Вложение скрыто, но удалить его с сервера не удалось: {error}"
                                                                )));
                                                            }
                                                        });
                                                    }
                                                }
                                            >
                                                {icon("close")}
                                            </button>
                                        </div>
                                    </For>
                                </Flex>
                            },
                        )
                    } else {
                        None
                    }
                }}

                <Flex style="gap: 8px; align-items: flex-end;">
                    <input
                        type="file"
                        accept=".txt,.md,.rs,.toml,.json,.sql,.js,.ts,.py,.go,.java,.c,.cpp,.h,.hpp,.cs,.rb,.php,.html,.css,.xml,.yaml,.yml,image/png,image/jpeg,image/webp"
                        style="display: none;"
                        id="file-input"
                        on:change=move |ev| {
                            use wasm_bindgen::JsCast;
                            let input: web_sys::HtmlInputElement = ev.target().unwrap().dyn_into().unwrap();
                            if let Some(files) = input.files() {
                                if let Some(file) = files.get(0) {
                                    let preview_url = file
                                        .type_()
                                        .starts_with("image/")
                                        .then(|| web_sys::Url::create_object_url_with_blob(&file).ok())
                                        .flatten();
                                    attach_file.run((file, preview_url));
                                }
                            }
                            // Clear input
                            input.set_value("");
                        }
                    />

                    <div style="flex: 1;">
                        <Textarea
                            value=vm.new_message
                            attr:id="llm-chat-composer"
                            placeholder="Введите сообщение... (Ctrl+Enter для отправки)"
                            attr:style="width: 100%; min-height: 60px; max-height: 200px; resize: vertical;"
                            disabled=vm.is_sending
                            on:keydown=move |ev: web_sys::KeyboardEvent| {
                                if ev.key() == "Enter" && ev.ctrl_key() {
                                    ev.prevent_default();
                                    handle_send.run(());
                                }
                            }
                        />
                    </div>

                    // Голосовой ввод: распознанный текст дописывается в поле ввода,
                    // дальше работает обычный handle_send. Компонент самодостаточен.
                    <DictationButton
                        target=vm.new_message
                        disabled=vm.is_sending
                        on_error=Callback::new(move |m: String| vm.error.set(Some(m)))
                    />

                    // Компактные иконочные кнопки (узкие по ширине): прикрепить и отправить.
                    <Button
                        appearance=ButtonAppearance::Secondary
                        disabled=vm.is_sending
                        attr:title="Прикрепить файл"
                        attr:style="min-width: 40px; padding-left: 8px; padding-right: 8px;"
                        on_click=move |_| {
                            if let Some(window) = web_sys::window() {
                                if let Some(document) = window.document() {
                                    if let Some(input) = document.get_element_by_id("file-input") {
                                        use wasm_bindgen::JsCast;
                                        if let Ok(input) = input.dyn_into::<web_sys::HtmlElement>() {
                                            input.click();
                                        }
                                    }
                                }
                            }
                        }
                    >
                        {icon("attach")}
                    </Button>

                    <Button
                        appearance=ButtonAppearance::Primary
                        disabled=vm.is_sending
                        attr:title=move || if vm.is_sending.get() { "Отправка…" } else { "Отправить" }
                        attr:style="min-width: 40px; padding-left: 8px; padding-right: 8px;"
                        on_click=move |_| handle_send.run(())
                    >
                        {icon("send")}
                    </Button>

                    // Компактные ссылки-подсказки в конце строки: микрофон и скриншот.
                    <div style="display: flex; align-items: center; gap: 12px; padding-bottom: 6px;">
                        <DictationDiagnostics />
                        <HintLink label="Скриншот?">
                            <div style="display: flex; flex-direction: column; gap: 8px; line-height: 1.5;">
                                <div style="font-weight: 600;">"Как вставить скриншот"</div>
                                <ol style="margin: 0; padding-left: 18px;">
                                    <li>"Нажмите Win+Shift+S и выделите область экрана."</li>
                                    <li>"Вернитесь сюда и нажмите Ctrl+V — откроется редактор."</li>
                                    <li>"Отметьте важное рамками/стрелками и нажмите «ОК»."</li>
                                </ol>
                                <div style="opacity: 0.75;">
                                    "Также можно приложить файл кнопкой скрепки слева."
                                </div>
                            </div>
                        </HintLink>
                    </div>
                </Flex>
                </div>
            </div>

            // Диалог диагностики: предопределённый разбор диалога моделью + опц.
            // комментарий. «Запустить проверку» закрывает диалог и отправляет промпт.
            <Dialog open=kb_open>
                <DialogSurface>
                    <DialogBody>
                        <DialogTitle>"Оформить знание в статью"</DialogTitle>
                        <DialogContent>
                            <div style="display: flex; flex-direction: column; gap: 8px;">
                                <span style="font-size: 13px; opacity: 0.7;">
                                    "Модель выделит из диалога одну устойчивую находку — вывод, правило \
                                     или объяснение расхождения — и создаст ЧЕРНОВИК статьи с тикетом \
                                     на проверку. Технические детали (SQL, схемы, поля) в базу знаний \
                                     не попадают."
                                </span>
                                <Textarea
                                    value=kb_comment
                                    placeholder="О чём именно статья, на что сделать акцент (необязательно)…"
                                    attr:style="width: 100%; min-height: 80px;"
                                    disabled=vm.is_sending
                                />
                                <div style="display: flex; align-items: center; gap: 8px;">
                                    <DictationButton
                                        target=kb_comment
                                        disabled=vm.is_sending
                                        on_error=Callback::new(move |m: String| vm.error.set(Some(m)))
                                    />
                                    <span style="font-size: 12px; opacity: 0.6;">"Голосовой ввод"</span>
                                </div>
                            </div>
                        </DialogContent>
                        <DialogActions>
                            <Button
                                appearance=ButtonAppearance::Secondary
                                on_click=move |_| kb_open.set(false)
                            >
                                "Отмена"
                            </Button>
                            <Button
                                appearance=ButtonAppearance::Primary
                                disabled=vm.is_sending
                                on_click=move |_| {
                                    let comment = kb_comment.get();
                                    let mut msg = String::from(KB_ARTICLE_PROMPT);
                                    if !comment.trim().is_empty() {
                                        msg.push_str("\n\nАкцент от пользователя: ");
                                        msg.push_str(comment.trim());
                                    }
                                    vm.new_message.set(msg);
                                    kb_open.set(false);
                                    kb_comment.set(String::new());
                                    handle_send.run(());
                                }
                            >
                                {icon("book-open-text")}
                                " Сформировать черновик"
                            </Button>
                        </DialogActions>
                    </DialogBody>
                </DialogSurface>
            </Dialog>
            <Dialog open=diag_open>
                <DialogSurface>
                    <DialogBody>
                        <DialogTitle>"Диагностика диалога"</DialogTitle>
                        <DialogContent>
                            <div style="display: flex; flex-direction: column; gap: 8px;">
                                <span style="font-size: 13px; opacity: 0.7;">
                                    "Модель разберёт диалог текстом (без вызова инструментов): что хотел \
                                     пользователь, какие шаги/ошибки были и что делать дальше."
                                </span>
                                <Textarea
                                    value=diag_comment
                                    placeholder="Комментарий или вопрос для проверки (необязательно)…"
                                    attr:style="width: 100%; min-height: 80px;"
                                    disabled=vm.is_sending
                                />
                                // Голосовой ввод комментария (обычный размер кнопки).
                                <div style="display: flex; align-items: center; gap: 8px;">
                                    <DictationButton
                                        target=diag_comment
                                        disabled=vm.is_sending
                                        on_error=Callback::new(move |m: String| vm.error.set(Some(m)))
                                    />
                                    <span style="font-size: 12px; opacity: 0.6;">"Голосовой ввод"</span>
                                </div>
                            </div>
                        </DialogContent>
                        <DialogActions>
                            <Button
                                appearance=ButtonAppearance::Secondary
                                on_click=move |_| diag_open.set(false)
                            >
                                "Отмена"
                            </Button>
                            <Button
                                appearance=ButtonAppearance::Primary
                                disabled=vm.is_sending
                                on_click=move |_| {
                                    let comment = diag_comment.get();
                                    let mut msg = String::from(DIAGNOSTIC_PROMPT);
                                    if !comment.trim().is_empty() {
                                        msg.push_str("\n\nКомментарий/вопрос пользователя: ");
                                        msg.push_str(comment.trim());
                                    }
                                    vm.new_message.set(msg);
                                    diag_open.set(false);
                                    diag_comment.set(String::new());
                                    handle_send.run(());
                                }
                            >
                                {icon("search")}
                                " Запустить проверку"
                            </Button>
                        </DialogActions>
                    </DialogBody>
                </DialogSurface>
            </Dialog>
            // Рабочий каталог задачи: анкета, план, журнал шагов. Открывается из «Ещё» —
            // показывает допущения модели до того, как по ним посчитают, и даёт
            // поправить анкету формой, что быстрее и точнее, чем диалогом.
            <ChatWorkspaceDrawer
                chat_id=chat_id_stored
                open=workspace_drawer_open
                workspace=workspace
                reload=reload_workspace
            />
            <ChatSettingsDialog open=settings_open prefs=ui_prefs />
            {move || pending_screenshot.get().map(|pending| view! {
                <ScreenshotEditor
                    source_file=pending.file
                    preview_url=pending.preview_url
                    on_cancel=cancel_screenshot
                    on_confirm=confirm_screenshot
                />
            })}
        </PageFrame>
    }
}
