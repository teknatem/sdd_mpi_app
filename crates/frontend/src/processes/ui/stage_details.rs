//! Страница Этапа отдельной вкладкой (`sys_stage_details_<code>`), admin-only.
//!
//! Каркас — тот же, что у страницы разработки плагина (`plugin_dev__<id>`):
//! `PageFrame` → `page__header` → `PageTabs` → `page__content` на вкладку
//! (UI-001…UI-003). Служебная обвязка (паспорт, статус, отпечаток, версии)
//! живёт на вкладке «Общее», а не в шапке; в шапке — только то, что меняет
//! состояние: сохранить, активировать, перечитать.
//!
//! Почему страница, а не карточка в списке. Этап адресуется ссылкой — из графа
//! Процесса в плагине (двойной клик по узлу), из журнала, из чата, — и на этот
//! адрес приходят **править**, а не только смотреть. Определения Этапов живут
//! в БД (ADR-0011 п.6), то есть правка мимо UI означает POST руками.
//!
//! Вкладки-редакторы — «Манифест» (JSON) и «Скрипт» (mjs): редактор на всю
//! высоту, единственный скролл живёт внутри CodeMirror. Редактор тот же, что у
//! плагинов ([`crate::plugins::editor::CodeEditor`]) — он про подсветку и
//! Ctrl+S, а не про плагины.
//!
//! Сохранение пишет **черновик**: у кода он один, повторное сохранение его
//! переписывает, новая версия заводится только поверх активной. Активация —
//! отдельной кнопкой и только для черновика (допуск в работу, ADR-0011 п.8).
//! Прогон при этом исполняет **сохранённую** версию, а не текст в редакторе.

use contracts::processes::{
    DefinitionStatus, DefinitionVersion, ProcessRecord, StageDefinition, StageManifest, StageRecord,
};
use leptos::prelude::*;
use leptos::task::spawn_local;

use super::super::api;
use super::parts::{
    definition_status_badge, definition_status_label, input_skeleton, short_digest,
};
use super::stage_card::{DryRunBlock, StageFacts};
use crate::plugins::editor::CodeEditor;
use crate::shared::components::page_tabs::{PageTabs, TabItem};
use crate::shared::page_frame::PageFrame;
use crate::shared::page_standard::PAGE_CAT_SYSTEM;
use crate::system::auth::guard::RequireAdmin;

#[component]
pub fn StageDetailsPage(code: String) -> impl IntoView {
    // children у RequireAdmin — Fn, поэтому code отдаём копией на каждый вызов.
    let code = StoredValue::new(code);
    view! {
        <RequireAdmin>
            <StageDetailsInner code=code.get_value() />
        </RequireAdmin>
    }
}

#[component]
fn StageDetailsInner(code: String) -> impl IntoView {
    let code = StoredValue::new(code);

    let record: RwSignal<Option<StageRecord>> = RwSignal::new(None);
    let versions: RwSignal<Vec<DefinitionVersion>> = RwSignal::new(Vec::new());
    // Паспорт читается только вместе с Процессами («где используется») и
    // каталогом Действий («чем меняет мир») — без них половина строк пустая.
    let processes: RwSignal<Vec<ProcessRecord>> = RwSignal::new(Vec::new());
    let actions: RwSignal<Vec<api::ActionInfo>> = RwSignal::new(Vec::new());

    let manifest_src = RwSignal::new(String::new());
    let script_src = RwSignal::new(String::new());

    let error: RwSignal<Option<String>> = RwSignal::new(None);
    let save_msg: RwSignal<Option<String>> = RwSignal::new(None);
    let saving = RwSignal::new(false);
    let loaded = RwSignal::new(false);
    let selected_tab = RwSignal::new("general");

    // Текст редакторов ведёт запись: правки, не сохранённые к моменту
    // перечитывания, теряются осознанно — иначе экран показывал бы одно, а
    // прогон и активация работали бы с другим.
    let apply = move |loaded_record: StageRecord| {
        manifest_src.set(pretty_manifest(&loaded_record.definition.manifest));
        script_src.set(loaded_record.definition.script.clone());
        record.set(Some(loaded_record));
    };

    let load_versions = move || {
        spawn_local(async move {
            match api::list_stage_versions(&code.get_value()).await {
                Ok(items) => versions.set(items),
                Err(message) => error.set(Some(message)),
            }
        });
    };

    // Открыть конкретную версию; `None` — головную (активную, иначе черновик).
    let load = move |version: Option<i32>| {
        spawn_local(async move {
            let wanted = code.get_value();
            let found = match version {
                Some(version) => api::get_stage(&wanted, version).await.map(Some),
                None => api::list_stages_full()
                    .await
                    .map(|items| items.into_iter().find(|item| item.code == wanted)),
            };
            match found {
                Ok(Some(item)) => {
                    error.set(None);
                    apply(item);
                }
                Ok(None) => error.set(Some(format!("Этап «{wanted}» не найден в каталоге."))),
                Err(message) => error.set(Some(message)),
            }
            loaded.set(true);
        });
        load_versions();
    };

    Effect::new(move |_| {
        load(None);
        spawn_local(async move {
            match api::list_processes_full().await {
                Ok(items) => processes.set(items),
                Err(message) => error.set(Some(message)),
            }
            match api::list_actions().await {
                Ok(items) => actions.set(items),
                Err(message) => error.set(Some(message)),
            }
        });
    });

    let save = Callback::new(move |_: ()| {
        if saving.get_untracked() {
            return;
        }
        let manifest: StageManifest = match serde_json::from_str(&manifest_src.get_untracked()) {
            Ok(manifest) => manifest,
            Err(parse_error) => {
                save_msg.set(Some(format!("Манифест не разобран: {parse_error}")));
                return;
            }
        };
        // Идентичность Этапа — код в манифесте: сохранение с чужим кодом завело
        // бы соседний Этап, а страница осталась бы на прежнем.
        if manifest.code != code.get_value() {
            save_msg.set(Some(format!(
                "Код в манифесте — «{}», страница открыта на «{}». Сохранение завело бы другой Этап.",
                manifest.code,
                code.get_value(),
            )));
            return;
        }
        let definition = StageDefinition {
            manifest,
            script: script_src.get_untracked(),
            // Отпечаток считает бэкенд: он выводится из содержимого.
            digest: String::new(),
        };

        saving.set(true);
        save_msg.set(None);
        spawn_local(async move {
            match api::save_stage(&definition).await {
                Ok(saved) => {
                    save_msg.set(Some(format!(
                        "Сохранено черновиком v{} ({})",
                        saved.version,
                        short_digest(&saved.digest),
                    )));
                    apply(saved);
                    load_versions();
                }
                Err(message) => save_msg.set(Some(format!("Ошибка: {message}"))),
            }
            saving.set(false);
        });
    });

    let activate = move |_| {
        let Some(current) = record.get_untracked() else {
            return;
        };
        if current.status != DefinitionStatus::Draft {
            return;
        }
        saving.set(true);
        save_msg.set(None);
        spawn_local(async move {
            match api::activate_stage(&current.code, current.version).await {
                Ok(activated) => {
                    save_msg.set(Some(format!("Активирована v{}", activated.version)));
                    apply(activated);
                    load_versions();
                }
                Err(message) => save_msg.set(Some(format!("Ошибка активации: {message}"))),
            }
            saving.set(false);
        });
    };

    let is_draft = Signal::derive(move || {
        record
            .get()
            .map(|item| item.status == DefinitionStatus::Draft)
            .unwrap_or(false)
    });

    view! {
        <PageFrame
            page_id="sys_stage_details--system"
            category=PAGE_CAT_SYSTEM
            class="sys-processes"
        >
            <div class="page__header">
                <div class="page__header-left">
                    <h1 class="page__title">
                        {move || match record.get() {
                            Some(item) => item.definition.manifest.title,
                            None if loaded.get() => "Этап".to_string(),
                            None => "Загрузка…".to_string(),
                        }}
                    </h1>
                    {move || record.get().map(|item| view! {
                        <span class=definition_status_badge(item.status)>
                            {definition_status_label(item.status)}
                        </span>
                        <span class="sys-processes__mono">
                            {format!("{} v{}", item.code, item.version)}
                        </span>
                    })}
                </div>
                <div class="page__header-right">
                    {move || save_msg.get().map(|message| view! {
                        <span class="page__header-meta">{message}</span>
                    })}
                    <button
                        class="button button--primary"
                        on:click=move |_| save.run(())
                        disabled=move || saving.get()
                        title="Сохранить манифест и скрипт черновиком (Ctrl+S в редакторе)"
                    >
                        {move || if saving.get() { "Сохранение…" } else { "Сохранить" }}
                    </button>
                    {move || is_draft.get().then(|| view! {
                        <button
                            class="button button--secondary"
                            on:click=activate
                            disabled=move || saving.get()
                            title="Допустить черновик в работу: прежняя активная версия уйдёт в архив"
                        >
                            "Активировать"
                        </button>
                    })}
                    <button
                        class="button button--ghost"
                        on:click=move |_| load(None)
                        title="Перечитать головную версию — несохранённые правки пропадут"
                    >
                        "Обновить"
                    </button>
                </div>
            </div>

            <PageTabs
                tabs=vec![
                    TabItem::new("general", "Общее"),
                    TabItem::new("manifest", "Манифест"),
                    TabItem::new("script", "Скрипт"),
                    TabItem::new("run", "Прогон"),
                    TabItem::new("versions", "Версии"),
                ]
                active=selected_tab.into()
                on_select=Callback::new(move |key: &'static str| selected_tab.set(key))
            />

            // ── Общее ────────────────────────────────────────────────────────
            <div
                class="page__content sys-processes__pane"
                class:sys-processes__hidden=move || selected_tab.get() != "general"
            >
                {move || error.get().map(|message| view! {
                    <div class="alert alert--error">{message}</div>
                })}
                {move || match record.get() {
                    Some(item) => {
                        let description = item.definition.manifest.description.clone();
                        let created = item.created_at.clone();
                        let author = item.created_by.clone().unwrap_or_else(|| "—".to_string());
                        let digest = short_digest(&item.digest);
                        let status = item.status;
                        let version = item.version;
                        let stage_code = item.code.clone();
                        view! {
                            <div class="sys-processes__card">
                                {(!description.is_empty())
                                    .then(|| view! { <p class="sys-processes__desc">{description}</p> })}

                                <dl class="sys-processes__info-grid">
                                    <dt class="sys-processes__info-term">"Код"</dt>
                                    <dd class="sys-processes__info-value sys-processes__mono">
                                        {stage_code}
                                    </dd>

                                    <dt class="sys-processes__info-term">"Версия"</dt>
                                    <dd class="sys-processes__info-value">
                                        {format!("v{version} · {}", definition_status_label(status))}
                                    </dd>

                                    <dt class="sys-processes__info-term">"Отпечаток"</dt>
                                    <dd class="sys-processes__info-value sys-processes__mono">
                                        {digest}
                                    </dd>

                                    <dt class="sys-processes__info-term">"Заведена"</dt>
                                    <dd class="sys-processes__info-value">
                                        {format!("{created}, автор {author}")}
                                    </dd>
                                </dl>

                                <StageFacts record=item processes=processes actions=actions />
                            </div>
                        }
                            .into_any()
                    }
                    None if loaded.get() => view! {
                        <div class="sys-processes__empty">
                            {format!("Этап «{}» не найден в каталоге.", code.get_value())}
                        </div>
                    }
                        .into_any(),
                    None => view! { <div class="sys-processes__empty">"Загрузка…"</div> }.into_any(),
                }}
            </div>

            // ── Манифест ─────────────────────────────────────────────────────
            <div
                class="page__content sys-processes__pane sys-processes__pane--editor"
                class:sys-processes__hidden=move || selected_tab.get() != "manifest"
            >
                <div class="sys-processes__editor-head">
                    <span class="sys-processes__editor-label">
                        "manifest: код, выходы графа, права (db:read: и action:), схема входа"
                    </span>
                </div>
                <CodeEditor
                    language="json"
                    value=manifest_src
                    on_save=save
                    class="plugin-code-editor--fill"
                />
            </div>

            // ── Скрипт ───────────────────────────────────────────────────────
            <div
                class="page__content sys-processes__pane sys-processes__pane--editor"
                class:sys-processes__hidden=move || selected_tab.get() != "script"
            >
                <div class="sys-processes__editor-head">
                    <span class="sys-processes__editor-label">
                        "script: ES-модуль QuickJS; export async function run(input, host) → { outcome, data }"
                    </span>
                </div>
                <CodeEditor
                    language="javascript"
                    value=script_src
                    on_save=save
                    class="plugin-code-editor--fill"
                />
            </div>

            // ── Прогон ───────────────────────────────────────────────────────
            <div
                class="page__content sys-processes__pane"
                class:sys-processes__hidden=move || selected_tab.get() != "run"
            >
                {move || match record.get() {
                    Some(item) => {
                        let skeleton = input_skeleton(item.definition.manifest.input_schema.as_ref());
                        view! {
                            <div class="sys-processes__note">
                                {format!(
                                    "Прогоняется сохранённая версия v{} — не текст в редакторе. \
                                     Сохраните правки, чтобы проверить их.",
                                    item.version,
                                )}
                            </div>
                            <DryRunBlock code=item.code version=item.version skeleton=skeleton />
                        }
                            .into_any()
                    }
                    None => view! { <div class="sys-processes__empty">"Загрузка…"</div> }.into_any(),
                }}
            </div>

            // ── Версии ───────────────────────────────────────────────────────
            <div
                class="page__content sys-processes__pane"
                class:sys-processes__hidden=move || selected_tab.get() != "versions"
            >
                <div class="sys-processes__note">
                    "Версия не удаляется, пока на неё ссылается экземпляр (ADR-0011 п.7). "
                    "Открытая версия правится в редакторах, а сохранение всегда пишет черновик."
                </div>
                <div class="table-wrapper">
                    <table class="table__data">
                        <thead>
                            <tr>
                                <th class="table__header-cell">"Версия"</th>
                                <th class="table__header-cell">"Состояние"</th>
                                <th class="table__header-cell">"Название"</th>
                                <th class="table__header-cell">"Отпечаток"</th>
                                <th class="table__header-cell">"Заведена"</th>
                                <th class="table__header-cell">"Автор"</th>
                                <th class="table__header-cell"></th>
                            </tr>
                        </thead>
                        <tbody>
                            <For
                                each=move || versions.get()
                                key=|item| format!("{}:{:?}", item.version, item.status)
                                let:item
                            >
                                {
                                    let version = item.version;
                                    let opened = Signal::derive(move || {
                                        record.get().map(|item| item.version) == Some(version)
                                    });
                                    view! {
                                        <tr class="table__row">
                                            <td class="table__cell table__cell--right">{version}</td>
                                            <td class="table__cell">
                                                <span class=definition_status_badge(item.status)>
                                                    {definition_status_label(item.status)}
                                                </span>
                                            </td>
                                            <td class="table__cell">{item.title.clone()}</td>
                                            <td class="table__cell sys-processes__mono">
                                                {short_digest(&item.digest)}
                                            </td>
                                            <td class="table__cell">{item.created_at.clone()}</td>
                                            <td class="table__cell">
                                                {item.created_by.clone().unwrap_or_else(|| "—".to_string())}
                                            </td>
                                            <td class="table__cell">
                                                <button
                                                    class="button button--ghost"
                                                    on:click=move |_| load(Some(version))
                                                    disabled=move || opened.get()
                                                >
                                                    {move || if opened.get() { "Открыта" } else { "Открыть" }}
                                                </button>
                                            </td>
                                        </tr>
                                    }
                                }
                            </For>
                        </tbody>
                    </table>
                </div>
            </div>
        </PageFrame>
    }
}

/// Манифест в редактор — с отступами: его правят руками, а строка в одну
/// линию правится только машиной.
fn pretty_manifest(manifest: &StageManifest) -> String {
    serde_json::to_string_pretty(manifest).unwrap_or_else(|error| format!("// {error}"))
}
