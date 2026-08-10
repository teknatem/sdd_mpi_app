//! Drawer «Файлы задачи» в деталях чата — дерево рабочего каталога.
//!
//! Смысл — сделать допущения модели видимыми до того, как по ним посчитают,
//! и дать поправить анкету формой: это быстрее и точнее, чем диалогом.
//! Показываем ровно то, что лежит на диске: задачи — каталоги верхнего уровня,
//! внутри активной — живые документы и подкаталог `steps`. Никаких карточек и
//! рамок: дерево читается быстрее, если его ничем не украшать.
//!
//! Уточняющие вопросы здесь не показываются: они требуют действия, поэтому
//! живут внизу ленты, над полем ввода (`questions_bar.rs`).

use leptos::prelude::*;
use leptos::task::spawn_local;
use thaw::*;

use super::model::{fetch_workspace_file, save_workspace_file, set_active_activity};
use contracts::domain::a018_llm_chat::workspace::{ChatFile, ChatWorkspaceView};

/// Человекочитаемая подпись живого документа — уходит в tooltip, чтобы в дереве
/// стояло настоящее имя файла.
fn file_hint(path: &str) -> &str {
    match path {
        "intake.md" => "Анкета задачи",
        "plan.md" => "План работы",
        "notes.md" => "Заметки",
        other => other,
    }
}

/// Класс-модификатор цвета по типу файла: живой документ, шаг журнала, прочее.
fn file_kind_class(path: &str) -> &'static str {
    match path.rsplit_once('.') {
        Some((_, "md")) => "chat-tree__file--md",
        Some((_, "json")) => "chat-tree__file--json",
        _ => "chat-tree__file--other",
    }
}

fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} Б")
    } else if bytes < 1024 * 1024 {
        format!("{:.0} КБ", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} МБ", bytes as f64 / (1024.0 * 1024.0))
    }
}

/// Разложить плоский список путей активной задачи на файлы её корня и
/// подкаталоги (сейчас это только `steps`, но группировка общая).
fn split_by_dir(files: Vec<ChatFile>) -> (Vec<ChatFile>, Vec<(String, Vec<ChatFile>)>) {
    let mut root = Vec::new();
    let mut dirs: Vec<(String, Vec<ChatFile>)> = Vec::new();
    for file in files {
        match file.path.split_once('/') {
            Some((dir, _)) => {
                let dir = dir.to_string();
                match dirs.iter_mut().find(|(name, _)| name == &dir) {
                    Some((_, bucket)) => bucket.push(file),
                    None => dirs.push((dir, vec![file])),
                }
            }
            None => root.push(file),
        }
    }
    (root, dirs)
}

#[component]
#[allow(non_snake_case)]
pub fn ChatWorkspaceDrawer(
    chat_id: StoredValue<String>,
    open: RwSignal<bool>,
    workspace: RwSignal<ChatWorkspaceView>,
    /// Перечитать каталог с сервера (владелец состояния — страница чата).
    reload: Callback<()>,
) -> impl IntoView {
    let error = RwSignal::new(Option::<String>::None);

    // Открытый файл: путь внутри задачи + содержимое в редакторе.
    let open_file = RwSignal::new(Option::<String>::None);
    let draft = RwSignal::new(String::new());
    let editable = RwSignal::new(false);
    let saving = RwSignal::new(false);
    let saved_note = RwSignal::new(Option::<String>::None);

    // Каталог меняется каждым ответом модели — перечитываем на каждое открытие,
    // а не один раз при монтировании.
    let was_open = RwSignal::new(false);
    Effect::new(move |_| {
        let is_open = open.get();
        if is_open && !was_open.get_untracked() {
            reload.run(());
        }
        was_open.set(is_open);
    });

    let active_name = move || {
        workspace
            .get()
            .activities
            .into_iter()
            .find(|a| a.is_active)
            .map(|a| a.name)
    };

    let open_doc = move |path: String| {
        let Some(activity) = active_name() else {
            return;
        };
        let id = chat_id.get_value();
        let full = format!("{}/{}", activity, path);
        open_file.set(Some(path));
        saved_note.set(None);
        spawn_local(async move {
            match fetch_workspace_file(&id, &full).await {
                Ok(file) => {
                    draft.set(file.content);
                    editable.set(file.is_live_document);
                    error.set(None);
                }
                Err(e) => error.set(Some(e)),
            }
        });
    };

    let save = move |_| {
        let (Some(path), Some(activity)) = (open_file.get_untracked(), active_name()) else {
            return;
        };
        let id = chat_id.get_value();
        let full = format!("{}/{}", activity, path);
        let content = draft.get_untracked();
        saving.set(true);
        spawn_local(async move {
            match save_workspace_file(&id, &full, &content).await {
                Ok(()) => {
                    saved_note.set(Some(
                        "Сохранено — подхватится следующим ответом".to_string(),
                    ));
                    error.set(None);
                }
                Err(e) => error.set(Some(e)),
            }
            saving.set(false);
        });
    };

    // Переключение активной задачи: каталог другой задачи раскрывается только
    // после переключения — бэкенд отдаёт файлы лишь для активной.
    let switch = move |name: String| {
        let id = chat_id.get_value();
        spawn_local(async move {
            match set_active_activity(&id, &name).await {
                Ok(()) => {
                    open_file.set(None);
                    draft.set(String::new());
                    reload.run(());
                }
                Err(e) => error.set(Some(e)),
            }
        });
    };

    // Одна строка файла: имя (цвет по типу) + размер.
    let file_row = move |file: ChatFile, depth: u8| {
        let path = file.path.clone();
        let name = path.rsplit('/').next().unwrap_or(&path).to_string();
        let is_open = {
            let path = path.clone();
            move || open_file.get().as_deref() == Some(path.as_str())
        };
        view! {
            <button
                class=format!("chat-tree__row chat-tree__file {}", file_kind_class(&path))
                class:chat-tree__row--open=is_open
                style=format!("padding-left: {}px;", 4 + depth as u32 * 16)
                title=file_hint(&path).to_string()
                on:click=move |_| open_doc(path.clone())
            >
                <span class="chat-tree__name">{name}</span>
                <span class="chat-tree__size">{format_bytes(file.bytes)}</span>
            </button>
        }
    };

    view! {
        <OverlayDrawer
            open=open
            position=DrawerPosition::Right
            size=DrawerSize::Medium
            close_on_esc=true
        >
            <DrawerHeader>
                <DrawerHeaderTitle>"Файлы задачи"</DrawerHeaderTitle>
            </DrawerHeader>
            <DrawerBody native_scrollbar=true>
                <div class="chat-workspace-drawer">
                    <Show when=move || error.get().is_some()>
                        <div class="chat-workspace__error">{move || error.get().unwrap_or_default()}</div>
                    </Show>

                    {move || {
                        let ws = workspace.get();
                        if ws.activities.is_empty() {
                            return view! {
                                <div class="chat-workspace__status">
                                    "Задач пока нет — они появятся, когда ассистент начнёт работу."
                                </div>
                            }
                            .into_any();
                        }
                        let (root_files, dirs) = split_by_dir(ws.files);
                        let plan_steps = ws.plan_steps.clone();
                        view! {
                            // План активной задачи структурой: статусы видно сразу,
                            // не открывая plan.md и не вчитываясь в markdown.
                            {(!plan_steps.is_empty())
                                .then(|| {
                                    view! {
                                        <div class="chat-workspace__plan">
                                            {plan_steps
                                                .into_iter()
                                                .map(|step| {
                                                    let mark = if step.done { "☑" } else { "☐" };
                                                    view! {
                                                        <div
                                                            class="chat-workspace__plan-step"
                                                            class:chat-workspace__plan-step--done=step.done
                                                            title=step.step_ref.clone().unwrap_or_default()
                                                        >
                                                            <span class="chat-workspace__plan-mark">{mark}</span>
                                                            <span class="chat-workspace__plan-id">{step.id.clone()}</span>
                                                            <span class="chat-workspace__plan-title">{step.title.clone()}</span>
                                                        </div>
                                                    }
                                                })
                                                .collect_view()}
                                        </div>
                                    }
                                })}
                            <div class="chat-tree">
                                {ws.activities
                                    .into_iter()
                                    .map(|activity| {
                                        let name = activity.name.clone();
                                        let switch_to = name.clone();
                                        let is_active = activity.is_active;
                                        let (root_files, dirs) = if is_active {
                                            (root_files.clone(), dirs.clone())
                                        } else {
                                            (Vec::new(), Vec::new())
                                        };
                                        view! {
                                            <button
                                                class="chat-tree__row chat-tree__dir"
                                                class:chat-tree__dir--active=is_active
                                                title=activity.description.clone()
                                                on:click=move |_| {
                                                    if !is_active {
                                                        switch(switch_to.clone());
                                                    }
                                                }
                                            >
                                                <span class="chat-tree__name">{format!("{name}/")}</span>
                                            </button>
                                            {root_files
                                                .into_iter()
                                                .map(|file| file_row(file, 1))
                                                .collect_view()}
                                            {dirs
                                                .into_iter()
                                                .map(|(dir, files)| {
                                                    view! {
                                                        <div
                                                            class="chat-tree__row chat-tree__dir chat-tree__dir--sub"
                                                            style="padding-left: 20px;"
                                                        >
                                                            <span class="chat-tree__name">{format!("{dir}/")}</span>
                                                        </div>
                                                        {files
                                                            .into_iter()
                                                            .map(|file| file_row(file, 2))
                                                            .collect_view()}
                                                    }
                                                })
                                                .collect_view()}
                                        }
                                    })
                                    .collect_view()}
                            </div>
                        }
                        .into_any()
                    }}

                    <Show when=move || open_file.get().is_some()>
                        <div class="chat-workspace__editor">
                            <div class="chat-workspace__editor-head">
                                {move || open_file.get().unwrap_or_default()}
                            </div>
                            <textarea
                                class="chat-workspace__textarea"
                                readonly=move || !editable.get()
                                prop:value=move || draft.get()
                                on:input=move |ev| draft.set(event_target_value(&ev))
                            ></textarea>
                            <Show when=move || editable.get()>
                                <div class="chat-workspace__editor-actions">
                                    <button
                                        class="chat-workspace__save"
                                        disabled=move || saving.get()
                                        on:click=save
                                    >
                                        {move || if saving.get() { "Сохраняю…" } else { "Сохранить" }}
                                    </button>
                                    <Show when=move || saved_note.get().is_some()>
                                        <span class="chat-workspace__saved">
                                            {move || saved_note.get().unwrap_or_default()}
                                        </span>
                                    </Show>
                                </div>
                            </Show>
                        </div>
                    </Show>
                </div>
            </DrawerBody>
        </OverlayDrawer>
    }
}
