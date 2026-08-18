//! ToolCallsTrace — компактный ряд пилюль вызовов инструментов в ответе ассистента.
//!
//! В сообщении хранится только минимум (`[{tool, ok, ms}]`) — его хватает на пилюли.
//! Полные детали (вход/выход/summary/stage) лежат в `sys_tool_trace` и подгружаются
//! лениво из `/api/a018-llm-chat/message/:message_id/tool-trace` при открытии
//! боковой панели деталей.

use super::api::fetch_tool_trace;
use crate::shared::clipboard::copy_to_clipboard;
use crate::shared::icons::icon;
use contracts::domain::a018_llm_chat::aggregate::ToolTraceEntry;
use leptos::prelude::*;
use thaw::*;
use wasm_bindgen_futures::spawn_local;

/// Минимальная запись пилюли из `tool_trace_json`.
#[derive(Debug, Clone)]
struct PillEntry {
    tool: String,
    ok: bool,
    ms: u64,
}

fn parse_pills(json: &str) -> Vec<PillEntry> {
    let Ok(arr) = serde_json::from_str::<serde_json::Value>(json) else {
        return vec![];
    };
    let Some(arr) = arr.as_array() else {
        return vec![];
    };
    arr.iter()
        .filter_map(|v| {
            Some(PillEntry {
                tool: v.get("tool")?.as_str()?.to_string(),
                ok: v.get("ok").and_then(|b| b.as_bool()).unwrap_or(true),
                ms: v.get("ms").and_then(|n| n.as_u64()).unwrap_or(0),
            })
        })
        .collect()
}

fn stage_label(stage: &str) -> &str {
    match stage {
        "discovery" => "1. Источник",
        "preview" => "2. Проверка",
        "publish" => "3. Публикация",
        _ => "Инструмент",
    }
}

/// Короткое имя инструмента для пилюли.
fn short_tool_name(name: &str) -> &str {
    match name {
        "list_entities" => "entities",
        "get_entity_schema" => "schema",
        "get_join_hint" => "join",
        "search_knowledge" => "search",
        "get_knowledge" => "knowledge",
        "list_data_sources" => "sources",
        "query_data_schema" => "data_schema",
        "run_data_view_scalar" => "data_value",
        "run_data_view_drilldown" => "data_rows",
        "execute_query" => "query",
        "create_drilldown_report" => "drilldown",
        other => other,
    }
}

fn format_value(value: &Option<serde_json::Value>) -> Option<String> {
    value
        .as_ref()
        .and_then(|value| serde_json::to_string_pretty(value).ok())
}

/// Кнопка «Копировать JSON»: пишет текст в буфер обмена и на пару секунд
/// показывает «Скопировано».
#[component]
#[allow(non_snake_case)]
fn CopyJsonButton(text: String) -> impl IntoView {
    let copied = RwSignal::new(false);
    let text = StoredValue::new(text);
    view! {
        <button
            class="tool-call__copy"
            title="Скопировать JSON"
            on:click=move |ev| {
                ev.stop_propagation();
                copy_to_clipboard(&text.get_value());
                copied.set(true);
                set_timeout(
                    move || copied.set(false),
                    std::time::Duration::from_millis(1500),
                );
            }
        >
            {icon("copy")}
            {move || if copied.get() { " Скопировано" } else { " Копировать" }}
        </button>
    }
}

/// Один узел дерева вызовов инструментов. По умолчанию свёрнут; клик по шапке
/// разворачивает детали (summary / вход / выход) с кнопками копирования JSON.
#[component]
#[allow(non_snake_case)]
fn ToolCallNode(entry: ToolTraceEntry) -> impl IntoView {
    let expanded = RwSignal::new(false);
    let ok = entry.ok;
    let index = format!("{}.{}", entry.iteration, entry.call_index);
    let stage = stage_label(&entry.stage).to_string();
    let name = entry.tool.clone();
    let ms = entry.ms;
    let summary = entry.summary.clone().unwrap_or_default();
    let input = format_value(&entry.input);
    let output = format_value(&entry.output);

    let root_class = if ok {
        "tool-call tool-call--ok"
    } else {
        "tool-call tool-call--err"
    };

    view! {
        <div class=root_class>
            <div
                class="tool-call__head"
                style="cursor: pointer;"
                on:click=move |_| expanded.update(|v| *v = !*v)
            >
                <span
                    class="tool-call__chevron"
                    style=move || format!(
                        "transform: rotate({});",
                        if expanded.get() { "90deg" } else { "0deg" },
                    )
                >
                    {icon("chevron-right")}
                </span>
                <span class="tool-call__index">{index}</span>
                <span class="tool-call__stage">{stage}</span>
                <span class="tool-call__name">{name}</span>
                <span class=if ok {
                    "tool-call__status tool-call__status--ok"
                } else {
                    "tool-call__status tool-call__status--err"
                }>
                    {if ok { "✓ Успешно" } else { "✗ Ошибка" }}
                </span>
                <span class="tool-call__ms">{format!("{}ms", ms)}</span>
            </div>

            <Show when=move || expanded.get()>
                {
                    let summary = summary.clone();
                    let input = input.clone();
                    let output = output.clone();
                    view! {
                        <div class="tool-call__details">
                            {(!summary.is_empty()).then(|| view! {
                                <div class="tool-call__summary">{summary.clone()}</div>
                            })}
                            {input.clone().map(|input| view! {
                                <div class="tool-call__io">
                                    <div class="tool-call__io-head">
                                        <span class="tool-call__io-label">"Вход"</span>
                                        <CopyJsonButton text=input.clone() />
                                    </div>
                                    <pre>{input}</pre>
                                </div>
                            })}
                            {output.clone().map(|output| view! {
                                <div class="tool-call__io">
                                    <div class="tool-call__io-head">
                                        <span class="tool-call__io-label">"Выход"</span>
                                        <CopyJsonButton text=output.clone() />
                                    </div>
                                    <pre>{output}</pre>
                                </div>
                            })}
                        </div>
                    }
                }
            </Show>
        </div>
    }
}

#[component]
#[allow(non_snake_case)]
pub fn ToolCallsTrace(tool_trace: Option<String>, message_id: String) -> impl IntoView {
    let Some(json) = tool_trace else {
        return view! { <></> }.into_any();
    };

    let pills = parse_pills(&json);
    if pills.is_empty() {
        return view! { <></> }.into_any();
    }

    let pills = StoredValue::new(pills);
    let message_id = StoredValue::new(message_id);

    // Ленивая загрузка полного журнала — только при первом открытии панели.
    let drawer_open = RwSignal::new(false);
    let loaded = RwSignal::new(false);
    let loading = RwSignal::new(false);
    let error = RwSignal::new(Option::<String>::None);
    let entries = RwSignal::new(Vec::<ToolTraceEntry>::new());

    let open_drawer = move |_| {
        drawer_open.set(true);
        if loaded.get_untracked() || loading.get_untracked() {
            return;
        }
        loading.set(true);
        error.set(None);
        let mid = message_id.get_value();
        spawn_local(async move {
            match fetch_tool_trace(&mid).await {
                Ok(rows) => {
                    entries.set(rows);
                    loaded.set(true);
                }
                Err(e) => error.set(Some(e)),
            }
            loading.set(false);
        });
    };

    view! {
        <div class="tool-trace">
            <div
                class="tool-trace__summary"
                on:click=open_drawer
                title="Нажмите чтобы посмотреть детали вызовов инструментов"
            >
                <span class="tool-trace__icon">{icon("wrench")}</span>
                <div class="tool-trace__pills">
                    {pills.get_value().iter().map(|e| {
                        let ok = e.ok;
                        let label = short_tool_name(&e.tool).to_string();
                        let ms = e.ms;
                        view! {
                            <span
                                class=if ok { "tool-trace__pill tool-trace__pill--ok" } else { "tool-trace__pill tool-trace__pill--err" }
                            >
                                {if ok { "✓ " } else { "✗ " }}
                                {label}
                                " "
                                <span class="tool-trace__ms">{format!("{}ms", ms)}</span>
                            </span>
                        }
                    }).collect::<Vec<_>>()}
                </div>
                <span class="tool-trace__toggle">{icon("chevron-right")}</span>
            </div>

            <OverlayDrawer
                open=drawer_open
                position=DrawerPosition::Right
                size=DrawerSize::Medium
                close_on_esc=true
            >
                <DrawerHeader>
                    <DrawerHeaderTitle>"Вызовы инструментов"</DrawerHeaderTitle>
                </DrawerHeader>
                <DrawerBody native_scrollbar=true>
                    {move || {
                        if loading.get() {
                            return view! { <div class="tool-trace__drawer-status">"Загрузка…"</div> }.into_any();
                        }
                        if let Some(err) = error.get() {
                            return view! {
                                <div class="tool-trace__drawer-status tool-trace__drawer-status--err">
                                    {format!("Ошибка загрузки: {}", err)}
                                </div>
                            }.into_any();
                        }
                        let rows = entries.get();
                        if rows.is_empty() {
                            return view! { <div class="tool-trace__drawer-status">"Нет детальных записей."</div> }.into_any();
                        }
                        view! {
                            <div class="tool-trace__calls">
                                {rows.into_iter().map(|e| view! { <ToolCallNode entry=e /> }).collect::<Vec<_>>()}
                            </div>
                        }.into_any()
                    }}
                </DrawerBody>
            </OverlayDrawer>
        </div>
    }.into_any()
}
