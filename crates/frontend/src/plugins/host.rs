//! Страница разработки плагина (`plugin_dev__<id>`).
//!
//! Вкладки-редакторы кода: «Клиент», «Сервер», «SQL», «Стили» (каждый редактор —
//! на всю высоту, один скролл). Плюс служебные: «Runner» (валидация + вызов
//! серверных методов) и «Статистика» (запуски/отклонения). Живой предпросмотр
//! вынесен в отдельную вкладку/окно [`crate::plugins::PluginView`] («▶ Запустить»),
//! чтобы править код и смотреть результат на двух мониторах: сохранить здесь →
//! Restart там подтягивает свежую версию.

pub(crate) mod model;

use self::model::{
    build_current_bundle, commit_selected_sql, default_run_context, first_sql_resource,
    format_invoke_body, health_badge, parse_context, pretty_context, sorted_resource_names,
};
use crate::layout::global_context::AppGlobalContext;
use crate::plugins::api;
use crate::plugins::editor::CodeEditor;
use crate::shared::change_tokens::ChangeTokenContext;
use crate::shared::date_utils::{format_space_naive_utc_local, format_utc_local};
use crate::shared::modal_frame::ModalFrame;
use contracts::plugins::{
    PluginDefinition, PluginInvokeRequest, PluginPublishResult, PluginStats, PluginStatus,
    PluginUpsert, PluginValidateReport,
};
use leptos::prelude::*;
use leptos::task::spawn_local;
use std::collections::HashMap;
use thaw::{Tab, TabList};

#[derive(Clone)]
struct ServerMethodExample {
    method: String,
    args: String,
}

/// Собрать актуальный bundle из редактируемых сигналов (для save / validate / runner).
/// Подпись и CSS-модификатор бейджа «здоровья» плагина.
/// Человекочитаемый вывод runner'а из полного тела ответа invoke
/// (результат либо ошибка со stage/stack + журнал host.log.*).
#[component]
pub fn PluginHost(plugin_id: String) -> impl IntoView {
    let ctx = use_context::<AppGlobalContext>().expect("AppGlobalContext not found");
    let change_tokens = use_context::<ChangeTokenContext>().expect("ChangeTokenContext not found");
    let (def, set_def) = signal(None::<PluginDefinition>);
    let (loading, set_loading) = signal(true);
    let (error, set_error) = signal(None::<String>);
    let client_src = RwSignal::new(String::new());
    let server_src = RwSignal::new(String::new());
    let styles_src = RwSignal::new(String::new());
    let sql_resources = RwSignal::new(HashMap::<String, String>::new());
    let selected_sql_name = RwSignal::new(None::<String>);
    let sql_name_input = RwSignal::new(String::new());
    let sql_src = RwSignal::new(String::new());
    let (version, set_version) = signal(1i32);
    // Редактируемый статус плагина (draft/active/disabled) — только на dev-странице (админ).
    let status = RwSignal::new("draft".to_string());
    let (saving, set_saving) = signal(false);
    let (save_msg, set_save_msg) = signal(None::<String>);
    let selected_tab = RwSignal::new("client".to_string());

    let runner_context = RwSignal::new("{}".to_string());
    let server_examples = RwSignal::new(Vec::<ServerMethodExample>::new());
    let runner_output = RwSignal::new(None::<String>);
    let runner_busy = RwSignal::new(false);
    let validate_report = RwSignal::new(None::<PluginValidateReport>);

    // Статистика запусков (вкладка «Статистика»).
    let stats = RwSignal::new(None::<PluginStats>);
    let stats_busy = RwSignal::new(false);
    let stats_error = RwSignal::new(None::<String>);

    // Публикация в S3.
    let show_publish_dialog = RwSignal::new(false);
    let (migration_version_current, set_migration_version_current) = signal(None::<i64>);

    let has_server = Signal::derive(move || !server_src.get().trim().is_empty());

    {
        let id = plugin_id.clone();
        spawn_local(async move {
            match api::get_by_id(&id).await {
                Ok(plugin) => {
                    client_src.set(plugin.bundle.client_script.clone().unwrap_or_default());
                    server_src.set(plugin.bundle.server_script.clone().unwrap_or_default());
                    styles_src.set(plugin.bundle.styles.clone().unwrap_or_default());
                    // Server-only плагин не имеет клиента — открываем сразу код сервера.
                    let client_empty = plugin
                        .bundle
                        .client_script
                        .as_deref()
                        .map(|s| s.trim().is_empty())
                        .unwrap_or(true);
                    let server_present = plugin
                        .bundle
                        .server_script
                        .as_deref()
                        .map(|s| !s.trim().is_empty())
                        .unwrap_or(false);
                    if client_empty && server_present {
                        selected_tab.set("server".to_string());
                    }
                    let default_context = default_run_context(&plugin.bundle);
                    runner_context.set(pretty_context(&default_context));
                    let resources = plugin.bundle.sql_resources.clone();
                    let (first_name, first_sql) = first_sql_resource(&resources);
                    sql_resources.set(resources);
                    selected_sql_name.set(first_name.clone());
                    sql_name_input.set(first_name.unwrap_or_default());
                    sql_src.set(first_sql);
                    set_version.set(plugin.version);
                    status.set(plugin.status.as_str().to_string());
                    set_def.set(Some(plugin));
                }
                Err(message) => set_error.set(Some(message)),
            }
            set_loading.set(false);
        });
    }

    spawn_local(async move {
        if let Ok(version) = api::migration_version().await {
            set_migration_version_current.set(Some(version));
        }
    });

    let save = {
        let id = plugin_id.clone();
        move |_| {
            let Some(current) = def.get_untracked() else {
                return;
            };
            let Some(bundle) = build_current_bundle(
                def,
                client_src,
                server_src,
                styles_src,
                sql_resources,
                selected_sql_name,
                sql_src,
            ) else {
                return;
            };
            let saved_bundle = bundle.clone();
            // Статус берём из редактируемого селектора (а не из текущего определения).
            let selected_status = status.get_untracked();
            let dto = PluginUpsert {
                id: Some(id.clone()),
                bundle,
                status: Some(selected_status.clone()),
                is_enabled: Some(current.is_enabled),
                owner_user_id: None,
                created_by_agent_id: None,
                version: Some(version.get_untracked()),
                capture_snapshot: saved_bundle.data.source.is_some(),
                allow_live_only: false,
            };

            set_saving.set(true);
            set_save_msg.set(None);
            spawn_local(async move {
                match api::upsert(&dto).await {
                    Ok(_) => {
                        set_version.update(|value| *value += 1);
                        set_def.update(|value| {
                            if let Some(plugin) = value {
                                plugin.bundle = saved_bundle;
                                plugin.version += 1;
                                plugin.status = PluginStatus::from_str(&selected_status);
                            }
                        });
                        set_save_msg.set(Some("Сохранено".to_string()));
                    }
                    Err(message) => set_save_msg.set(Some(format!("Ошибка: {message}"))),
                }
                set_saving.set(false);
            });
        }
    };

    let select_sql = move |event| {
        commit_selected_sql(sql_resources, selected_sql_name, sql_src);
        let name = event_target_value(&event);
        let source = sql_resources
            .with_untracked(|items| items.get(&name).cloned())
            .unwrap_or_default();
        selected_sql_name.set(Some(name.clone()));
        sql_name_input.set(name);
        sql_src.set(source);
    };

    let add_sql = move |_| {
        commit_selected_sql(sql_resources, selected_sql_name, sql_src);
        let name = sql_resources.with_untracked(|items| {
            (1..)
                .map(|index| format!("query{index}"))
                .find(|candidate| !items.contains_key(candidate))
                .unwrap()
        });
        sql_resources.update(|items| {
            items.insert(name.clone(), "SELECT 1 AS value".to_string());
        });
        selected_sql_name.set(Some(name.clone()));
        sql_name_input.set(name);
        sql_src.set("SELECT 1 AS value".to_string());
        set_save_msg.set(None);
    };

    let rename_sql = move |_| {
        let Some(old_name) = selected_sql_name.get_untracked() else {
            return;
        };
        let new_name = sql_name_input.get_untracked().trim().to_string();
        if new_name.is_empty() {
            set_save_msg.set(Some("Имя SQL-ресурса не может быть пустым".to_string()));
            return;
        }
        if new_name != old_name
            && sql_resources.with_untracked(|items| items.contains_key(&new_name))
        {
            set_save_msg.set(Some(format!("SQL-ресурс '{new_name}' уже существует")));
            return;
        }

        commit_selected_sql(sql_resources, selected_sql_name, sql_src);
        if new_name != old_name {
            sql_resources.update(|items| {
                let source = items.remove(&old_name).unwrap_or_default();
                items.insert(new_name.clone(), source);
            });
            selected_sql_name.set(Some(new_name.clone()));
            sql_name_input.set(new_name);
        }
        set_save_msg.set(None);
    };

    let delete_sql = move |_| {
        let Some(name) = selected_sql_name.get_untracked() else {
            return;
        };
        sql_resources.update(|items| {
            items.remove(&name);
        });
        let next_name =
            sql_resources.with_untracked(|items| sorted_resource_names(items).into_iter().next());
        let next_source = next_name
            .as_ref()
            .and_then(|next| sql_resources.with_untracked(|items| items.get(next).cloned()))
            .unwrap_or_default();
        selected_sql_name.set(next_name.clone());
        sql_name_input.set(next_name.unwrap_or_default());
        sql_src.set(next_source);
        set_save_msg.set(None);
    };

    let run_validate = move |_| {
        let Some(bundle) = build_current_bundle(
            def,
            client_src,
            server_src,
            styles_src,
            sql_resources,
            selected_sql_name,
            sql_src,
        ) else {
            return;
        };
        runner_busy.set(true);
        runner_output.set(None);
        spawn_local(async move {
            match api::validate(&bundle).await {
                Ok(report) => {
                    let exports = report.server_exports.clone();
                    server_examples.update(|examples| {
                        let previous = examples
                            .iter()
                            .map(|example| (example.method.clone(), example.args.clone()))
                            .collect::<HashMap<_, _>>();
                        *examples = exports
                            .into_iter()
                            .map(|method| ServerMethodExample {
                                args: previous
                                    .get(&method)
                                    .cloned()
                                    .unwrap_or_else(|| "{}".to_string()),
                                method,
                            })
                            .collect();
                    });
                    validate_report.set(Some(report));
                }
                Err(message) => runner_output.set(Some(format!("Ошибка валидации: {message}"))),
            }
            runner_busy.set(false);
        });
    };

    let invoke_plugin_id = plugin_id.clone();
    let run_invoke = Callback::new(move |(method, args_source): (String, String)| {
        let method = method.trim().to_string();
        if method.is_empty() {
            runner_output.set(Some("Укажите имя метода".to_string()));
            return;
        }
        let args: serde_json::Value = match serde_json::from_str(&args_source) {
            Ok(value) => value,
            Err(error) => {
                runner_output.set(Some(format!("Некорректный JSON аргументов: {error}")));
                return;
            }
        };
        let context = match parse_context(&runner_context.get_untracked()) {
            Ok(context) => context,
            Err(message) => {
                runner_output.set(Some(message));
                return;
            }
        };
        let request = PluginInvokeRequest {
            method,
            args,
            context,
            data_mode: contracts::plugins::PluginDataMode::Live,
        };
        let id = invoke_plugin_id.clone();
        runner_busy.set(true);
        runner_output.set(None);
        spawn_local(async move {
            let text = match api::invoke_raw(&id, &request).await {
                Ok(body) => format_invoke_body(&body),
                Err(message) => format!("Сетевая ошибка: {message}"),
            };
            runner_output.set(Some(text));
            runner_busy.set(false);
        });
    });

    let export_plugin_id = plugin_id.clone();
    let export_plugin = move |_| {
        let url = format!("/api/plugin/{}/export", export_plugin_id);
        let fallback = def
            .get_untracked()
            .map(|d| format!("{}.plugin.zip", d.bundle.manifest.code))
            .unwrap_or_else(|| "plugin.zip".to_string());
        spawn_local(async move {
            if let Err(message) =
                crate::shared::auth_download::download_authenticated_file(&url, &fallback).await
            {
                set_error.set(Some(format!("Экспорт не удался: {message}")));
            }
        });
    };

    let view_plugin_id = plugin_id.clone();
    let open_view = move |_| {
        let title = def
            .get_untracked()
            .map(|p| p.bundle.manifest.title)
            .unwrap_or_else(|| "Плагин".to_string());
        ctx.open_tab(&format!("plugin__{}", view_plugin_id), &title);
    };

    let stats_plugin_id = plugin_id.clone();
    let load_stats = Callback::new(move |_: ()| {
        if stats_busy.get_untracked() {
            return;
        }
        let id = stats_plugin_id.clone();
        stats_busy.set(true);
        stats_error.set(None);
        spawn_local(async move {
            match api::get_stats(&id, 7).await {
                Ok(value) => stats.set(Some(value)),
                Err(message) => stats_error.set(Some(message)),
            }
            stats_busy.set(false);
        });
    });

    // Ленивая загрузка при первом открытии вкладки «Статистика».
    Effect::new(move |_| {
        if selected_tab.get() == "stats"
            && stats.get_untracked().is_none()
            && !stats_busy.get_untracked()
        {
            load_stats.run(());
        }
    });

    view! {
        <div class="plugin-host plugin-host--dev">
            {move || loading.get().then(|| view! {
                <div class="plugin-host__state">"Загрузка плагина..."</div>
            })}
            {move || error.get().map(|message| view! {
                <div class="plugin-host__alert plugin-host__alert--error">{message}</div>
            })}

            <div class="plugin-host__header">
                <div class="plugin-host__title-row">
                    <h2 class="plugin-host__title">
                        {move || def.get().map(|plugin| plugin.bundle.manifest.title).unwrap_or_default()}
                    </h2>
                    <span class="badge badge--primary">"Разработка"</span>
                    <span class="plugin-host__code">
                        {move || def.get().map(|plugin| plugin.bundle.manifest.code).unwrap_or_default()}
                    </span>
                    // Статус плагина — применяется при сохранении (только на dev-странице).
                    <label class="plugin-host__status" title="Статус плагина (применяется при сохранении)">
                        "Статус: "
                        <select
                            class="plugin-host__status-select"
                            prop:value=move || status.get()
                            on:change=move |ev| status.set(event_target_value(&ev))
                        >
                            <option value="draft">"Черновик"</option>
                            <option value="active">"Активен"</option>
                            <option value="disabled">"Отключён"</option>
                        </select>
                    </label>
                    <button
                        class="plugin-host__run"
                        on:click=save
                        disabled=Signal::derive(move || saving.get())
                        title="Сохранить код, SQL, стили и статус"
                    >
                        {move || if saving.get() { "Сохранение..." } else { "Сохранить" }}
                    </button>
                    {move || save_msg.get().map(|message| view! {
                        <span class="plugin-host__save-msg">{message}</span>
                    })}
                    <button
                        class="plugin-host__run plugin-host__run--server plugin-host__export"
                        on:click=open_view
                        title="Открыть плагин в отдельной вкладке/окне (Restart там подтянет свежую версию)"
                    >
                        "▶ Запустить"
                    </button>
                    <button
                        class="plugin-host__run plugin-host__run--server"
                        on:click=export_plugin
                    >
                        "Экспорт .zip"
                    </button>
                    <button
                        class="plugin-host__run plugin-host__run--server"
                        on:click=move |_| show_publish_dialog.set(true)
                    >
                        "Опубликовать в S3"
                    </button>
                </div>
                {move || def.get()
                    .and_then(|plugin| plugin.bundle.manifest.description)
                    .map(|description| view! { <p class="plugin-host__desc">{description}</p> })}
                {move || def.get().map(|plugin| {
                    let s3 = match plugin.s3_published_version {
                        Some(v) if plugin.s3_published_version == Some(plugin.version) => {
                            format!("в S3 v{v} (актуально)")
                        }
                        Some(v) => format!("в S3 v{v} (есть несохранённые/неопубликованные правки)"),
                        None => "в S3 не публиковался".to_string(),
                    };
                    view! {
                        <p class="plugin-host__desc text-muted" style="font-size: 12px;">
                            {format!("Версия: локально v{} · {s3}", plugin.version)}
                        </p>
                    }
                })}
                {move || {
                    let built_for = def.get()
                        .and_then(|plugin| plugin.bundle.manifest.built_for_migration)
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "—".to_string());
                    let current = migration_version_current.get()
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "…".to_string());
                    view! {
                        <p class="plugin-host__desc text-muted" style="font-size: 12px;">
                            {format!("Миграция БД: {current} / плагин рассчитан на: {built_for}")}
                        </p>
                    }
                }}
            </div>

            <PluginPublishDialog
                plugin_id=plugin_id.clone()
                show=show_publish_dialog
                def=def
                set_def=set_def
                plugins_token=change_tokens.plugins
            />

            <div class="plugin-host__tabs">
                <TabList selected_value=selected_tab>
                    <Tab value="client".to_string()>"Клиент"</Tab>
                    <Tab value="server".to_string()>"Сервер"</Tab>
                    <Tab value="sql".to_string()>"SQL"</Tab>
                    <Tab value="styles".to_string()>"Стили"</Tab>
                    <Tab value="runner".to_string()>"Runner"</Tab>
                    <Tab value="stats".to_string()>"Статистика"</Tab>
                </TabList>
            </div>

            <div
                class="plugin-host__pane plugin-host__pane--editor"
                class:plugin-host__hidden=move || selected_tab.get() != "client"
            >
                <div class="plugin-host__editor-head">
                    <div class="plugin-host__editor-label">
                        "client_script: ES-модуль в iframe; export async function mount(root, host)"
                    </div>
                </div>
                <CodeEditor
                    language="javascript"
                    value=client_src
                    class="plugin-code-editor--fill"
                />
            </div>

            <div
                class="plugin-host__pane plugin-host__pane--editor"
                class:plugin-host__hidden=move || selected_tab.get() != "server"
            >
                <div class="plugin-host__editor-head">
                    <div class="plugin-host__editor-label">
                        "server_script: ES-модуль QuickJS; экспортированные async-функции"
                    </div>
                </div>
                <CodeEditor
                    language="javascript"
                    value=server_src
                    class="plugin-code-editor--fill"
                />
            </div>

            <div
                class="plugin-host__pane plugin-host__pane--editor"
                class:plugin-host__hidden=move || selected_tab.get() != "sql"
            >
                <div class="plugin-host__editor-head">
                    <div class="plugin-host__editor-label">
                        "SQL-ресурсы: host.db.queryResource(name, params)"
                    </div>
                    <div class="plugin-host__resource-toolbar">
                        <select
                            class="plugin-host__resource-select"
                            prop:value=move || selected_sql_name.get().unwrap_or_default()
                            on:change=select_sql
                        >
                            {move || sorted_resource_names(&sql_resources.get())
                                .into_iter()
                                .map(|name| view! {
                                    <option value=name.clone()>{name.clone()}</option>
                                })
                                .collect_view()}
                        </select>
                        <input
                            class="plugin-host__resource-name"
                            placeholder="Имя SQL-ресурса"
                            prop:value=move || sql_name_input.get()
                            on:input=move |event| sql_name_input.set(event_target_value(&event))
                        />
                        <button type="button" class="plugin-host__resource-action" on:click=rename_sql>
                            "Переименовать"
                        </button>
                        <button type="button" class="plugin-host__resource-action" on:click=add_sql>
                            "Добавить"
                        </button>
                        <button
                            type="button"
                            class="plugin-host__resource-action plugin-host__resource-action--danger"
                            on:click=delete_sql
                            disabled=Signal::derive(move || selected_sql_name.get().is_none())
                        >
                            "Удалить"
                        </button>
                    </div>
                </div>
                {move || if selected_sql_name.get().is_some() {
                    view! {
                        <CodeEditor
                            language="sql"
                            value=sql_src
                            class="plugin-code-editor--fill"
                        />
                    }.into_any()
                } else {
                    view! {
                        <div class="plugin-host__state">
                            "SQL-ресурсов пока нет. Нажмите «Добавить»."
                        </div>
                    }.into_any()
                }}
            </div>

            <div
                class="plugin-host__pane plugin-host__pane--editor"
                class:plugin-host__hidden=move || selected_tab.get() != "styles"
            >
                <div class="plugin-host__editor-head">
                    <div class="plugin-host__editor-label">"styles: CSS внутри iframe"</div>
                </div>
                <CodeEditor
                    language="css"
                    value=styles_src
                    class="plugin-code-editor--fill"
                />
            </div>

            <div
                class="plugin-host__pane"
                class:plugin-host__hidden=move || selected_tab.get() != "runner"
            >
                <div class="plugin-host__toolbar">
                    <button
                        class="plugin-host__run plugin-host__run--server"
                        on:click=run_validate
                        disabled=Signal::derive(move || runner_busy.get())
                    >
                        "Refresh methods"
                    </button>
                </div>
                {move || (!has_server.get()).then(|| view! {
                    <div class="plugin-host__state">
                        "У плагина нет server_script. Добавьте его на вкладке «Код»."
                    </div>
                })}
                {move || validate_report.get().map(|report| {
                    let exports = report.server_exports.join(", ");
                    let errors = report.errors.clone();
                    view! {
                        <div class="plugin-host__card plugin-host__runner-report">
                            <div class="plugin-host__runner-line">
                                <span class="plugin-host__runner-key">"Статус: "</span>
                                {if report.ok { "✓ валиден" } else { "✗ ошибки" }}
                            </div>
                            <div class="plugin-host__runner-line">
                                <span class="plugin-host__runner-key">"Экспорты: "</span>
                                {if exports.is_empty() { "—".to_string() } else { exports }}
                            </div>
                            {(!errors.is_empty()).then(|| view! {
                                <div class="plugin-host__runner-errors">
                                    {errors.into_iter().map(|e| view! {
                                        <div class="plugin-host__runner-line">
                                            {format!("[{}] {}", e.stage, e.message)}
                                        </div>
                                    }).collect_view()}
                                </div>
                            })}
                        </div>
                    }
                })}
                <div class="plugin-host__runner-form">
                    <label class="plugin-host__runner-field">
                        "Context JSON"
                        <textarea
                            class="plugin-host__runner-args plugin-host__runner-args--context"
                            prop:value=move || runner_context.get()
                            on:input=move |e| runner_context.set(event_target_value(&e))
                        ></textarea>
                    </label>
                </div>
                {move || {
                    let examples = server_examples.get();
                    if examples.is_empty() {
                        view! {
                            <div class="plugin-host__state">
                                "Click refresh methods to build editable server call examples."
                            </div>
                        }.into_any()
                    } else {
                        view! {
                            <div class="plugin-host__method-list">
                                {examples.into_iter().map(|example| {
                                    let method = example.method.clone();
                                    let method_for_run = method.clone();
                                    view! {
                                        <div class="plugin-host__method-card">
                                            <div class="plugin-host__method-head">
                                                <span class="plugin-host__method-name">{method.clone()}</span>
                                                <button
                                                    class="plugin-host__run"
                                                    on:click=move |_| {
                                                        let args = server_examples
                                                            .with_untracked(|items| {
                                                                items
                                                                    .iter()
                                                                    .find(|item| item.method == method_for_run)
                                                                    .map(|item| item.args.clone())
                                                                    .unwrap_or_else(|| "{}".to_string())
                                                            });
                                                        run_invoke.run((method_for_run.clone(), args));
                                                    }
                                                    disabled=Signal::derive(move || runner_busy.get())
                                                >
                                                    {move || if runner_busy.get() { "Running..." } else { "Run" }}
                                                </button>
                                            </div>
                                            <label class="plugin-host__runner-field">
                                                "Args JSON"
                                                <textarea
                                                    class="plugin-host__runner-args"
                                                    prop:value=example.args.clone()
                                                    on:input=move |event| {
                                                        let value = event_target_value(&event);
                                                        let method = method.clone();
                                                        server_examples.update(|items| {
                                                            if let Some(item) = items.iter_mut().find(|item| item.method == method) {
                                                                item.args = value;
                                                            }
                                                        });
                                                    }
                                                ></textarea>
                                            </label>
                                        </div>
                                    }
                                }).collect_view()}
                            </div>
                        }.into_any()
                    }
                }}
                {move || runner_output.get().map(|text| view! {
                    <pre class="plugin-host__runner-output">{text}</pre>
                })}
            </div>

            <div
                class="plugin-host__pane"
                class:plugin-host__hidden=move || selected_tab.get() != "stats"
            >
                <div class="plugin-host__toolbar">
                    <button
                        class="plugin-host__run plugin-host__run--server"
                        on:click=move |_| load_stats.run(())
                        disabled=Signal::derive(move || stats_busy.get())
                    >
                        {move || if stats_busy.get() { "Загрузка..." } else { "Обновить (7 дней)" }}
                    </button>
                </div>
                {move || stats_error.get().map(|message| view! {
                    <div class="plugin-host__alert plugin-host__alert--error">{message}</div>
                })}
                {move || stats.get().map(|data| {
                    let s = data.summary;
                    let (label, modifier) = health_badge(s.health);
                    let recent = data.recent;
                    view! {
                        <div class="plugin-host__card plugin-host__stats">
                            <div class="plugin-host__stats-grid">
                                <div class="plugin-host__stat">
                                    <span class="plugin-host__runner-key">"Здоровье"</span>
                                    <span class=format!("badge badge--{modifier}")>{label}</span>
                                </div>
                                <div class="plugin-host__stat">
                                    <span class="plugin-host__runner-key">"Запусков (7д)"</span>{s.total}
                                </div>
                                <div class="plugin-host__stat">
                                    <span class="plugin-host__runner-key">"Доля ошибок"</span>
                                    {format!("{} ({:.0}%)", s.errors, s.error_rate * 100.0)}
                                </div>
                                <div class="plugin-host__stat">
                                    <span class="plugin-host__runner-key">"Таймауты"</span>{s.timeouts}
                                </div>
                                <div class="plugin-host__stat">
                                    <span class="plugin-host__runner-key">"avg / max"</span>
                                    {format!("{} / {} мс", s.avg_ms, s.max_ms)}
                                </div>
                            </div>
                            {(!s.by_stage.is_empty()).then(|| view! {
                                <div class="plugin-host__runner-line">
                                    <span class="plugin-host__runner-key">"Ошибки по стадиям: "</span>
                                    {s.by_stage.into_iter()
                                        .map(|sc| format!("{}×{}", sc.stage, sc.count))
                                        .collect::<Vec<_>>()
                                        .join(", ")}
                                </div>
                            })}
                            {if recent.is_empty() {
                                view! { <div class="plugin-host__state">"Запусков пока нет."</div> }.into_any()
                            } else {
                                view! {
                                    <table class="plugin-host__table">
                                        <thead>
                                            <tr>
                                                <th>"Время"</th>
                                                <th>"Метод"</th>
                                                <th>"Статус"</th>
                                                <th class="plugin-host__num">"мс"</th>
                                                <th class="plugin-host__num">"строк"</th>
                                                <th>"Стадия"</th>
                                            </tr>
                                        </thead>
                                        <tbody>
                                            {recent.into_iter().map(|r| view! {
                                                <tr>
                                                    <td>{format_space_naive_utc_local(&r.started_at, "%Y-%m-%d %H:%M:%S")}</td>
                                                    <td>{r.method}</td>
                                                    <td>{r.status}</td>
                                                    <td class="plugin-host__num">{r.duration_ms}</td>
                                                    <td class="plugin-host__num">
                                                        {r.row_count.map(|v| v.to_string()).unwrap_or_default()}
                                                    </td>
                                                    <td>{r.error_stage.unwrap_or_default()}</td>
                                                </tr>
                                            }).collect_view()}
                                        </tbody>
                                    </table>
                                }.into_any()
                            }}
                        </div>
                    }
                })}
            </div>
        </div>
    }
}

/// Модальное подтверждение публикации текущей сохранённой версии плагина в S3.
#[component]
fn PluginPublishDialog(
    plugin_id: String,
    show: RwSignal<bool>,
    def: ReadSignal<Option<PluginDefinition>>,
    set_def: WriteSignal<Option<PluginDefinition>>,
    /// Frontend change-token плагинов — бампим после публикации, чтобы список
    /// (вкладка `plugins`, колонка «На сервере») перечитался сам.
    plugins_token: RwSignal<u64>,
) -> impl IntoView {
    let plugin_id = StoredValue::new(plugin_id);
    let (publishing, set_publishing) = signal(false);
    let (result, set_result) = signal(None::<Result<PluginPublishResult, String>>);

    // Текущая сохранённая версия уже опубликована в S3 → публиковать нечего.
    let already_published = Signal::derive(move || {
        def.get()
            .map(|p| p.s3_published_version == Some(p.version))
            .unwrap_or(false)
    });

    let close = Callback::new(move |_: ()| {
        set_result.set(None);
        set_publishing.set(false);
        show.set(false);
    });

    let publish = move |_| {
        let id = plugin_id.get_value();
        set_publishing.set(true);
        set_result.set(None);
        spawn_local(async move {
            let outcome = api::publish(&id).await;
            if let Ok(published) = &outcome {
                // Отражаем факт публикации в локальном состоянии, иначе строка
                // «Ранее опубликовано…» и кнопка остаются устаревшими — диалог
                // повторно предлагает опубликовать уже опубликованную версию.
                let published = published.clone();
                set_def.update(|value| {
                    if let Some(plugin) = value {
                        plugin.s3_published_version = Some(published.version);
                        plugin.s3_published_at = Some(published.uploaded_at);
                    }
                });
                // Пусть открытая вкладка списка плагинов подхватит новую версию S3.
                plugins_token.update(|v| *v += 1);
            }
            set_result.set(Some(outcome));
            set_publishing.set(false);
        });
    };

    view! {
        <Show when=move || show.get() fallback=|| view! {}>
            <ModalFrame on_close=close modal_style="max-width: 480px; width: 92vw;".to_string()>
                <div class="modal-header">
                    <span class="modal-title">"Публикация плагина в S3"</span>
                </div>

                <div class="modal-body" style="display: flex; flex-direction: column; gap: 12px;">
                    {move || def.get().map(|plugin| {
                        let previous = match (plugin.s3_published_version, plugin.s3_published_at) {
                            (Some(v), Some(at)) => {
                                format!("Ранее опубликовано: v{v} от {}", format_utc_local(&at, "%Y-%m-%d %H:%M"))
                            }
                            _ => "Ещё не публиковался в S3".to_string(),
                        };
                        view! {
                            <div>
                                <div style="font-weight: 600;">{plugin.bundle.manifest.title.clone()}</div>
                                <div class="text-muted" style="font-size: 13px;">
                                    {plugin.bundle.manifest.code.clone()}
                                </div>
                            </div>
                            <div style="font-size: 13px;">
                                {format!("Публикуется последняя сохранённая версия: v{}", plugin.version)}
                            </div>
                            <div class="text-muted" style="font-size: 13px;">{previous}</div>
                        }
                    })}

                    {move || already_published.get().then(|| view! {
                        <div class="text-muted" style="font-size: 13px;">
                            "Текущая версия уже опубликована в S3. Сохраните изменения, чтобы опубликовать новую версию."
                        </div>
                    })}

                    {move || result.get().map(|outcome| match outcome {
                        Ok(published) => view! {
                            <div class="plugins-alert plugins-alert--info">
                                {format!("Опубликовано: v{} от {}", published.version, format_utc_local(&published.uploaded_at, "%Y-%m-%d %H:%M"))}
                            </div>
                        }.into_any(),
                        Err(err) => view! { <div class="plugins-alert plugins-alert--error">{err}</div> }.into_any(),
                    })}
                </div>

                <div class="modal-footer">
                    <button class="button button--secondary" on:click=move |_| close.run(()) disabled=publishing>
                        "Закрыть"
                    </button>
                    <button
                        class="button button--primary"
                        on:click=publish
                        disabled=move || publishing.get() || already_published.get()
                    >
                        {move || if publishing.get() { "Публикация…" } else { "Опубликовать" }}
                    </button>
                </div>
            </ModalFrame>
        </Show>
    }
}
