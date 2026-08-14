//! Страница-реестр плагинов: список всех плагинов с ключевыми данными,
//! ссылкой открытия (вкладка `plugin__<id>`) и кнопкой создания JS-примера.

use crate::layout::global_context::AppGlobalContext;
use crate::plugins::api;
use crate::shared::change_tokens::ChangeTokenContext;
use crate::shared::date_utils::format_utc_local;
use crate::shared::modal_frame::ModalFrame;
use crate::shared::page_frame::PageFrame;
use crate::shared::page_standard::PAGE_CAT_LIST;
use contracts::plugins::{PluginCatalog, PluginCatalogEntry, PluginListItem, PluginUpdateStatus};
use leptos::prelude::*;
use leptos::task::spawn_local;
use std::collections::HashMap;
use thaw::*;
use wasm_bindgen::JsCast;

/// Оценка плагина: 5 звёзд, только для просмотра (изменяется на странице плагина).
fn rating_stars_readonly(rating: Option<i32>) -> impl IntoView {
    let current = rating.unwrap_or(0);
    view! {
        <span style="display: inline-flex; gap: 1px; font-size: 14px; line-height: 1;" title="Оценка плагина">
            {(1..=5)
                .map(|n| {
                    let filled = n <= current;
                    view! {
                        <span style=move || format!(
                            "color:{};",
                            if filled { "#f5a623" } else { "var(--color-text-secondary, #9ca3af)" }
                        )>
                            {if filled { "★" } else { "☆" }}
                        </span>
                    }
                })
                .collect_view()}
        </span>
    }
}

#[component]
pub fn PluginList() -> impl IntoView {
    let ctx = use_context::<AppGlobalContext>().expect("AppGlobalContext not found");
    let change_tokens = use_context::<ChangeTokenContext>().expect("ChangeTokenContext not found");
    // Админ управляет плагинами (видит все статусы, импорт/публикацию/обновления);
    // обычный пользователь только использует активные плагины.
    let user_is_admin = crate::system::auth::context::is_admin();

    let (items, set_items) = signal(Vec::<PluginListItem>::new());
    let (loading, set_loading) = signal(true);
    let (error, set_error) = signal(None::<String>);
    let (creating, set_creating) = signal(false);
    let (import_msg, set_import_msg) = signal(None::<String>);
    let (updates, set_updates) = signal(HashMap::<String, PluginUpdateStatus>::new());
    let selected_update = RwSignal::new(None::<(String, PluginUpdateStatus)>);
    let active_tab = RwSignal::new("installed".to_string());
    let (catalog, set_catalog) = signal(PluginCatalog::new());
    let (catalog_error, set_catalog_error) = signal(None::<String>);
    let (installing, set_installing) = signal(None::<String>);
    let (install_error, set_install_error) = signal(None::<String>);

    let reload = move || {
        set_loading.set(true);
        set_error.set(None);
        spawn_local(async move {
            // Админ видит все плагины (любой статус); пользователь — только активные включённые.
            let result = if user_is_admin {
                api::list_all().await
            } else {
                api::list_enabled().await
            };
            match result {
                Ok(list) => set_items.set(list),
                Err(e) => set_error.set(Some(e)),
            }
            set_loading.set(false);
        });
        // Сверка с S3 и полный каталог — только для админа (эндпоинты admin-only).
        if user_is_admin {
            // Проверка обновлений в S3 подгружается параллельно; если S3 не настроен —
            // это не критично, колонка «Обновление» просто останется пустой.
            spawn_local(async move {
                if let Ok(rows) = api::check_updates().await {
                    set_updates.set(
                        rows.into_iter()
                            .map(|row| (row.plugin_id.clone(), row))
                            .collect(),
                    );
                }
            });
            // Полный каталог S3 — для вкладки «Доступные плагины».
            spawn_local(async move {
                match api::get_catalog().await {
                    Ok(entries) => {
                        set_catalog.set(entries);
                        set_catalog_error.set(None);
                    }
                    Err(e) => set_catalog_error.set(Some(e)),
                }
            });
        }
    };

    // Первичная загрузка.
    reload();

    // Реагируем на изменения плагинов из других вкладок/страниц (публикация, обновление).
    Effect::new(move |prev: Option<u64>| {
        let token = change_tokens.plugins.get();
        if prev.is_some() && prev != Some(token) {
            reload();
        }
        token
    });

    let create_test = move |_| {
        set_creating.set(true);
        spawn_local(async move {
            match api::insert_test_data().await {
                Ok(()) => {
                    set_creating.set(false);
                    reload();
                }
                Err(e) => {
                    set_error.set(Some(e));
                    set_creating.set(false);
                }
            }
        });
    };

    let handle_import = move |ev: web_sys::Event| {
        let Some(input) = ev
            .target()
            .and_then(|t| t.dyn_into::<web_sys::HtmlInputElement>().ok())
        else {
            return;
        };
        let Some(file) = input.files().and_then(|files| files.get(0)) else {
            return;
        };
        // Сброс значения, чтобы повторный выбор того же файла снова сработал.
        input.set_value("");
        set_error.set(None);
        set_import_msg.set(Some("Импорт…".to_string()));
        spawn_local(async move {
            let buffer = match wasm_bindgen_futures::JsFuture::from(file.array_buffer()).await {
                Ok(buffer) => buffer,
                Err(e) => {
                    set_error.set(Some(format!("Не удалось прочитать файл: {e:?}")));
                    set_import_msg.set(None);
                    return;
                }
            };
            let array = js_sys::Uint8Array::new(&buffer);
            let mut bytes = vec![0u8; array.length() as usize];
            array.copy_to(&mut bytes);

            match api::import_archive(bytes).await {
                Ok(body) => {
                    let code = body
                        .get("code")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    if body.get("ok").and_then(|v| v.as_bool()).unwrap_or(false) {
                        set_import_msg.set(Some(format!("Импортирован «{code}» (статус draft)")));
                        reload();
                    } else {
                        let errors = body
                            .get("validate")
                            .and_then(|v| v.get("errors"))
                            .map(|e| e.to_string())
                            .unwrap_or_default();
                        set_error.set(Some(format!("«{code}» не прошёл валидацию: {errors}")));
                        set_import_msg.set(None);
                    }
                }
                Err(message) => {
                    set_error.set(Some(message));
                    set_import_msg.set(None);
                }
            }
        });
    };

    let install_plugin = move |code: String| {
        set_installing.set(Some(code.clone()));
        set_install_error.set(None);
        spawn_local(async move {
            match api::install_from_catalog(&code).await {
                Ok(()) => {
                    set_installing.set(None);
                    reload();
                }
                Err(e) => {
                    set_install_error.set(Some(e));
                    set_installing.set(None);
                }
            }
        });
    };

    view! {
        <PageFrame page_id="plugins--list" category=PAGE_CAT_LIST class="plugins-page">
            <div class="plugins-page__header">
                <div class="plugins-page__heading">
                    <h1 class="plugins-page__title">"Плагины"</h1>
                    <p class="plugins-page__subtitle">
                        "Реестр расширений платформы. "
                        {move || {
                            let n = items.get().len();
                            format!("Всего: {}", n)
                        }}
                    </p>
                </div>
                <div class="plugins-page__actions">
                    <button class="plugins-btn plugins-btn--ghost" on:click=move |_| reload()>
                        "Обновить"
                    </button>
                    // Импорт и создание примеров — управление плагинами, только для админа.
                    {user_is_admin.then(|| view! {
                        <label class="plugins-btn plugins-btn--ghost plugins-import">
                            "⭳ Импорт .zip"
                            <input
                                type="file"
                                accept=".zip,application/zip"
                                class="plugins-import__input"
                                on:change=handle_import
                            />
                        </label>
                        <button
                            class="plugins-btn plugins-btn--primary"
                            on:click=create_test
                            disabled=Signal::derive(move || creating.get())
                        >
                            {move || if creating.get() {
                                "Создание…"
                            } else {
                                "＋ Создать пример JS-плагина"
                            }}
                        </button>
                    })}
                </div>
            </div>

            // Вкладка «Доступные» (каталог S3) — только для админа; пользователь видит
            // лишь установленные активные плагины.
            {user_is_admin.then(|| view! {
                <div style="margin-bottom: 16px;">
                    <TabList selected_value=active_tab>
                        <Tab value="installed".to_string()>"Установленные"</Tab>
                        <Tab value="available".to_string()>
                            {move || {
                                let cat = catalog.get();
                                let local_codes: std::collections::HashSet<String> =
                                    items.get().iter().map(|p| p.code.clone()).collect();
                                let n = cat.keys().filter(|code| !local_codes.contains(*code)).count();
                                if n > 0 {
                                    format!("Доступные ({n})")
                                } else {
                                    "Доступные".to_string()
                                }
                            }}
                        </Tab>
                    </TabList>
                </div>
            })}

            <div class="plugins-page__content" style=move || if active_tab.get() == "installed" { "" } else { "display: none;" }>
                {move || error.get().map(|e| view! {
                    <div class="plugins-alert plugins-alert--error">{e}</div>
                })}

                {move || import_msg.get().map(|m| view! {
                    <div class="plugins-alert plugins-alert--info">{m}</div>
                })}

                {move || loading.get().then(|| view! {
                    <div class="plugins-page__state">"Загрузка…"</div>
                })}

                {move || {
                    let list = items.get();
                    let upd = updates.get();
                    if loading.get() {
                        ().into_any()
                    } else if list.is_empty() {
                        view! {
                            <div class="plugins-empty">
                                <div class="plugins-empty__icon">"🧩"</div>
                                <div class="plugins-empty__title">"Плагинов пока нет"</div>
                                <div class="plugins-empty__hint">
                                    "Нажмите «Создать пример JS-плагина», чтобы добавить демонстрационный отчёт."
                                </div>
                            </div>
                        }.into_any()
                    } else {
                        view! {
                            <div class="plugins-table-wrap">
                                <table class="table__data plugins-table">
                                    <thead>
                                        <tr>
                                            <th>"Код"</th>
                                            <th>"Название"</th>
                                            <th>"Исполнение"</th>
                                            <th>"Статус"</th>
                                            <th>"Версия"</th>
                                            <th>"На сервере"</th>
                                            <th>"Оценка"</th>
                                            <th>"Обновлён"</th>
                                            <th class="plugins-table__action-col">"Действие"</th>
                                        </tr>
                                    </thead>
                                    <tbody>
                                        {list.into_iter().map(|p| {
                                            // Название → пользовательская страница плагина (`plugin__<id>`).
                                            let view_key = format!("plugin__{}", p.id);
                                            let view_title = p.title.clone();
                                            // Колонка «Действие» для пользователя («Открыть») —
                                            // тоже рантайм-страница (отдельные строки, т.к. `view_*` уходят в заголовок).
                                            let open_key = format!("plugin__{}", p.id);
                                            let open_title = p.title.clone();
                                            // «Изменить» → страница редактирования (`plugin_dev__<id>`).
                                            let edit_key = format!("plugin_dev__{}", p.id);
                                            let edit_title = format!("{} — разработка", p.title);
                                            let updated = format_utc_local(&p.updated_at, "%Y-%m-%d %H:%M");
                                            let status = p.status.clone();
                                            let status_mod = format!(
                                                "badge badge--{}",
                                                match status.as_str() {
                                                    "active" => "success",
                                                    "draft" => "warning",
                                                    _ => "neutral",
                                                }
                                            );
                                            let local_version = p.version;
                                            let update_status = upd.get(&p.id).cloned();
                                            let title_for_dialog = p.title.clone();
                                            view! {
                                                <tr>
                                                    <td><span class="plugins-code">{p.code.clone()}</span></td>
                                                    <td class="plugins-table__title">
                                                        <a
                                                            href="#"
                                                            class="plugins-link"
                                                            on:click=move |ev| {
                                                                ev.prevent_default();
                                                                ctx.open_tab(&view_key, &view_title);
                                                            }
                                                        >
                                                            {p.title.clone()}
                                                        </a>
                                                    </td>
                                                    <td><span class="badge badge--primary">{p.runtime.clone()}</span></td>
                                                    <td><span class=status_mod>{status}</span></td>
                                                    <td>{format!("v{local_version}")}</td>
                                                    <td>
                                                        {match update_status.filter(|u| u.remote_version.is_some()) {
                                                            Some(u) if u.update_available => {
                                                                let date = u.remote_uploaded_at
                                                                    .map(|dt| format_utc_local(&dt, "%Y-%m-%d %H:%M"))
                                                                    .unwrap_or_default();
                                                                let title_for_click = title_for_dialog.clone();
                                                                let remote_v = u.remote_version.unwrap_or_default();
                                                                view! {
                                                                    <div class="plugins-server-cell">
                                                                        <span
                                                                            class="badge badge--warning"
                                                                            title="В S3 доступна более новая версия"
                                                                        >
                                                                            {format!("v{remote_v} · {date}")}
                                                                        </span>
                                                                        <button
                                                                            class="plugins-btn plugins-btn--primary plugins-btn--sm"
                                                                            title="Установить версию из S3"
                                                                            on:click=move |_| {
                                                                                selected_update.set(Some((title_for_click.clone(), u.clone())));
                                                                            }
                                                                        >
                                                                            "Установить"
                                                                        </button>
                                                                    </div>
                                                                }.into_any()
                                                            }
                                                            Some(u) => {
                                                                // Опубликовано в S3, версия не новее локальной — просто версия/дата, без действия.
                                                                let date = u.remote_uploaded_at
                                                                    .map(|dt| format_utc_local(&dt, "%Y-%m-%d %H:%M"))
                                                                    .unwrap_or_default();
                                                                view! {
                                                                    <span class="plugins-table__muted" title="Актуальная версия опубликована в S3">
                                                                        {format!("v{} · {}", u.remote_version.unwrap_or_default(), date)}
                                                                    </span>
                                                                }.into_any()
                                                            }
                                                            None => view! { <span class="plugins-table__muted">"—"</span> }.into_any(),
                                                        }}
                                                    </td>
                                                    <td>{rating_stars_readonly(p.rating)}</td>
                                                    <td class="plugins-table__muted">{updated}</td>
                                                    <td class="plugins-table__action-col">
                                                        {if user_is_admin {
                                                            // Админ → страница разработки (`plugin_dev__<id>`).
                                                            view! {
                                                                <a
                                                                    href="#"
                                                                    class="plugins-link"
                                                                    on:click=move |ev| {
                                                                        ev.prevent_default();
                                                                        ctx.open_tab(&edit_key, &edit_title);
                                                                    }
                                                                >
                                                                    "Изменить"
                                                                </a>
                                                            }.into_any()
                                                        } else {
                                                            // Пользователь → рантайм-страница плагина (`plugin__<id>`).
                                                            view! {
                                                                <a
                                                                    href="#"
                                                                    class="plugins-link"
                                                                    on:click=move |ev| {
                                                                        ev.prevent_default();
                                                                        ctx.open_tab(&open_key, &open_title);
                                                                    }
                                                                >
                                                                    "Открыть"
                                                                </a>
                                                            }.into_any()
                                                        }}
                                                    </td>
                                                </tr>
                                            }
                                        }).collect_view()}
                                    </tbody>
                                </table>
                            </div>
                        }.into_any()
                    }
                }}
            </div>

            <div class="plugins-page__content" style=move || if active_tab.get() == "available" { "" } else { "display: none;" }>
                {move || catalog_error.get().map(|e| view! {
                    <div class="plugins-alert plugins-alert--error">{format!("Каталог S3 недоступен: {e}")}</div>
                })}

                {move || install_error.get().map(|e| view! {
                    <div class="plugins-alert plugins-alert--error">{e}</div>
                })}

                {move || {
                    let cat = catalog.get();
                    let local_codes: std::collections::HashSet<String> =
                        items.get().iter().map(|p| p.code.clone()).collect();
                    let mut available: Vec<(String, PluginCatalogEntry)> = cat
                        .into_iter()
                        .filter(|(code, _)| !local_codes.contains(code))
                        .collect();
                    available.sort_by(|a, b| a.1.title.cmp(&b.1.title));

                    if available.is_empty() {
                        view! {
                            <div class="plugins-empty">
                                <div class="plugins-empty__icon">"☁️"</div>
                                <div class="plugins-empty__title">"Новых плагинов нет"</div>
                                <div class="plugins-empty__hint">
                                    "Все опубликованные в S3 плагины уже установлены локально."
                                </div>
                            </div>
                        }.into_any()
                    } else {
                        view! {
                            <div class="plugins-table-wrap">
                                <table class="table__data plugins-table">
                                    <thead>
                                        <tr>
                                            <th>"Код"</th>
                                            <th>"Название"</th>
                                            <th>"Версия"</th>
                                            <th>"Опубликован"</th>
                                            <th class="plugins-table__action-col">"Действие"</th>
                                        </tr>
                                    </thead>
                                    <tbody>
                                        {available.into_iter().map(|(code, entry)| {
                                            let date = format_utc_local(&entry.uploaded_at, "%Y-%m-%d %H:%M");
                                            let code_for_disabled = code.clone();
                                            let code_for_click = code.clone();
                                            view! {
                                                <tr>
                                                    <td><span class="plugins-code">{code.clone()}</span></td>
                                                    <td class="plugins-table__title">{entry.title.clone()}</td>
                                                    <td>{format!("v{}", entry.version)}</td>
                                                    <td class="plugins-table__muted">{date}</td>
                                                    <td class="plugins-table__action-col">
                                                        <button
                                                            class="plugins-btn plugins-btn--primary"
                                                            disabled=Signal::derive(move || installing.get().is_some())
                                                            on:click=move |_| install_plugin(code_for_click.clone())
                                                        >
                                                            {move || if installing.get().as_deref() == Some(code_for_disabled.as_str()) {
                                                                "Установка…"
                                                            } else {
                                                                "Установить"
                                                            }}
                                                        </button>
                                                    </td>
                                                </tr>
                                            }
                                        }).collect_view()}
                                    </tbody>
                                </table>
                            </div>
                        }.into_any()
                    }
                }}
            </div>

            <PluginUpdateDialog selected=selected_update on_updated=Callback::new(move |_| reload()) />
        </PageFrame>
    }
}

/// Модальное подтверждение применения обновления плагина, скачанного из S3.
#[component]
fn PluginUpdateDialog(
    selected: RwSignal<Option<(String, PluginUpdateStatus)>>,
    on_updated: Callback<()>,
) -> impl IntoView {
    let (applying, set_applying) = signal(false);
    let (result, set_result) = signal(None::<Result<(), String>>);

    let close = Callback::new(move |_: ()| {
        set_result.set(None);
        set_applying.set(false);
        selected.set(None);
    });

    let apply = move |_| {
        let Some((_, status)) = selected.get_untracked() else {
            return;
        };
        set_applying.set(true);
        set_result.set(None);
        spawn_local(async move {
            match api::apply_update(&status.plugin_id, status.remote_version).await {
                Ok(()) => {
                    on_updated.run(());
                    // Обновление применено — закрываем диалог (список перечитан через on_updated).
                    close.run(());
                }
                Err(err) => set_result.set(Some(Err(err))),
            }
            set_applying.set(false);
        });
    };

    view! {
        <Show when=move || selected.get().is_some() fallback=|| view! {}>
            <ModalFrame on_close=close modal_style="max-width: 480px; width: 92vw;".to_string()>
                <div class="modal-header">
                    <span class="modal-title">"Обновление плагина"</span>
                </div>

                <div class="modal-body" style="display: flex; flex-direction: column; gap: 12px;">
                    {move || selected.get().map(|(title, status)| {
                        let remote_date = status.remote_uploaded_at
                            .map(|dt| format_utc_local(&dt, "%Y-%m-%d %H:%M"))
                            .unwrap_or_default();
                        view! {
                            <div>
                                <div style="font-weight: 600;">{title}</div>
                                <div class="text-muted" style="font-size: 13px;">{status.code.clone()}</div>
                            </div>
                            <div style="font-size: 13px; display: flex; flex-direction: column; gap: 4px;">
                                <div>"Локальная версия: " <b>{format!("v{}", status.local_version)}</b></div>
                                <div>
                                    "Версия в S3: "
                                    <b>{format!("v{}", status.remote_version.unwrap_or_default())}</b>
                                    " от " {remote_date}
                                </div>
                            </div>
                        }
                    })}

                    {move || result.get().map(|outcome| match outcome {
                        Ok(()) => view! { <div class="plugins-alert plugins-alert--info">"Обновление применено"</div> }.into_any(),
                        Err(err) => view! { <div class="plugins-alert plugins-alert--error">{err}</div> }.into_any(),
                    })}
                </div>

                <div class="modal-footer">
                    <button class="button button--secondary" on:click=move |_| close.run(()) disabled=applying>
                        "Отмена"
                    </button>
                    <button class="button button--primary" on:click=apply disabled=applying>
                        {move || if applying.get() { "Обновление…" } else { "Обновить" }}
                    </button>
                </div>
            </ModalFrame>
        </Show>
    }
}
