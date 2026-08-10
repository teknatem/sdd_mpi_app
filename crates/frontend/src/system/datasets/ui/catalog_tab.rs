//! Вкладка «Восстановление»: снапшоты из каталога S3 и запуск предпросмотра.

use std::collections::HashSet;

use contracts::system::datasets::{RestoreMode, RestorePreviewDto, RestorePreviewRequest};
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::shared::date_utils::{format_bytes_compact, format_datetime_utc_local};
use crate::shared::icons::icon;
use crate::system::datasets::api;
use crate::system::datasets::ui::diagnostics_tab::env_badge_class;
use crate::system::datasets::ui::diff_dialog::DiffDialog;
use crate::system::datasets::ui::{DatasetsState, DATETIME_FMT};

#[component]
pub fn CatalogTab(
    state: DatasetsState,
    catalog_error: RwSignal<Option<String>>,
) -> impl IntoView {
    let expanded = RwSignal::<Option<String>>::new(None);
    let selected_sets = RwSignal::<HashSet<String>>::new(HashSet::new());
    let mode = RwSignal::new(RestoreMode::Merge);
    let preview = RwSignal::<Option<RestorePreviewDto>>::new(None);

    let expand = move |snapshot_id: String, set_ids: Vec<String>| {
        if expanded.get().as_deref() == Some(snapshot_id.as_str()) {
            expanded.set(None);
            return;
        }
        selected_sets.set(set_ids.into_iter().collect());
        expanded.set(Some(snapshot_id));
    };

    let show_diff = move |snapshot_id: String| {
        let set_ids: Vec<String> = selected_sets.get().into_iter().collect();
        if set_ids.is_empty() {
            state
                .error
                .set(Some("Не выбрано ни одного набора".to_string()));
            return;
        }
        let request = RestorePreviewRequest {
            snapshot_id,
            set_ids,
            mode: mode.get(),
        };
        state.busy_label.set(Some("Сравнение с локальными данными...".to_string()));
        state.error.set(None);
        spawn_local(async move {
            match api::restore_preview(&request).await {
                Ok(result) => preview.set(Some(result)),
                Err(err) => state.error.set(Some(err)),
            }
            state.busy_label.set(None);
        });
    };

    let delete_snapshot = move |snapshot_id: String| {
        let confirmed = web_sys::window()
            .and_then(|window| {
                window
                    .confirm_with_message(&format!(
                        "Удалить снапшот {snapshot_id} из бакета? Действие необратимо."
                    ))
                    .ok()
            })
            .unwrap_or(false);
        if !confirmed {
            return;
        }
        state.busy_label.set(Some("Удаление снапшота...".to_string()));
        spawn_local(async move {
            match api::delete_snapshot(&snapshot_id).await {
                Ok(()) => {
                    state.notice.set(Some(format!("Снапшот {snapshot_id} удалён")));
                    state.reload.run(());
                }
                Err(err) => state.error.set(Some(err)),
            }
            state.busy_label.set(None);
        });
    };

    let download = move |snapshot_id: String| {
        state.busy_label.set(Some("Подготовка архива...".to_string()));
        spawn_local(async move {
            match api::download_snapshot_bytes(&snapshot_id).await {
                Ok(bytes) => {
                    if let Err(err) = trigger_download(&format!("{snapshot_id}.zip"), &bytes) {
                        state.error.set(Some(err));
                    }
                }
                Err(err) => state.error.set(Some(err)),
            }
            state.busy_label.set(None);
        });
    };

    view! {
        <section class="datasets__section">
            <h2 class="datasets__section-title">"Снапшоты в S3"</h2>

            {move || catalog_error.get().map(|err| view! {
                <div class="alert alert--warning">{err}</div>
            })}

            {move || {
                let Some(catalog) = state.catalog.get() else {
                    return view! { <div class="text-muted">"Каталог недоступен."</div> }.into_any();
                };
                if catalog.snapshots.is_empty() {
                    return view! {
                        <div class="text-muted">"В бакете пока нет ни одного снапшота."</div>
                    }.into_any();
                }
                let local_id = catalog.local_instance_id.clone();

                view! {
                    <div class="datasets__snapshots">
                        {catalog.snapshots.into_iter().map(|snapshot| {
                            let snapshot_id = snapshot.snapshot_id.clone();
                            let is_own = snapshot.source.instance_id == local_id;
                            let set_ids = snapshot.set_ids();
                            let total = snapshot.total_bytes();
                            let files = snapshot.file_count();
                            let expand_id = snapshot_id.clone();
                            let expand_sets = set_ids.clone();

                            view! {
                                <div class="datasets__snapshot" class:datasets__snapshot--own=is_own>
                                    <div class="datasets__snapshot-head">
                                        <button
                                            class="datasets__snapshot-toggle"
                                            on:click=move |_| expand(expand_id.clone(), expand_sets.clone())
                                        >
                                            {icon("chevron-right")}
                                        </button>
                                        <div class="datasets__snapshot-main">
                                            <div class="datasets__snapshot-title">
                                                <span class="datasets__instance-label">
                                                    {snapshot.source.instance_label.clone()}
                                                </span>
                                                <span class=env_badge_class(snapshot.source.instance_env)>
                                                    {snapshot.source.instance_env.label_ru()}
                                                </span>
                                                {is_own.then(|| view! {
                                                    <span class="badge badge--info">"этот экземпляр"</span>
                                                })}
                                                <span class="text-muted">
                                                    {format_datetime_utc_local(&snapshot.created_at, DATETIME_FMT)}
                                                </span>
                                            </div>
                                            <div class="datasets__snapshot-meta">
                                                <span class="datasets__mono">{snapshot_id.clone()}</span>
                                                <span class="text-muted">{snapshot.created_by.clone()}</span>
                                                <span class="text-muted">
                                                    {format!("{} файлов · {}", files, format_bytes_compact(total))}
                                                </span>
                                                <span class="text-muted">
                                                    {format!("схема БД {}", snapshot.source.schema_version)}
                                                </span>
                                            </div>
                                            {snapshot.note.clone().map(|note| view! {
                                                <div class="datasets__snapshot-note">{note}</div>
                                            })}
                                            <div class="datasets__chips">
                                                {snapshot.sets.iter().map(|set| view! {
                                                    <span class="datasets__chip">
                                                        {format!("{} · {}", set.label_ru, format_bytes_compact(set.total_bytes))}
                                                    </span>
                                                }).collect_view()}
                                            </div>
                                        </div>
                                        <div class="datasets__snapshot-actions">
                                            <button
                                                class="button button--ghost"
                                                title="Скачать архив"
                                                on:click={
                                                    let id = snapshot_id.clone();
                                                    move |_| download(id.clone())
                                                }
                                            >
                                                {icon("download")}
                                            </button>
                                            <button
                                                class="button button--ghost"
                                                title="Удалить снапшот"
                                                on:click={
                                                    let id = snapshot_id.clone();
                                                    move |_| delete_snapshot(id.clone())
                                                }
                                            >
                                                {icon("trash-2")}
                                            </button>
                                        </div>
                                    </div>

                                    <Show when={
                                        let id = snapshot_id.clone();
                                        move || expanded.get().as_deref() == Some(id.as_str())
                                    }>
                                        <div class="datasets__snapshot-body">
                                            <div class="datasets__restore-sets">
                                                {snapshot.sets.iter().map(|set| {
                                                    let set_id = set.set_id.clone();
                                                    let checkbox_id = set_id.clone();
                                                    view! {
                                                        <label class="form__checkbox-wrapper">
                                                            <input
                                                                type="checkbox"
                                                                class="form__checkbox"
                                                                prop:checked=move || selected_sets.get().contains(&checkbox_id)
                                                                on:change={
                                                                    let set_id = set_id.clone();
                                                                    move |_| selected_sets.update(|current| {
                                                                        if !current.remove(&set_id) {
                                                                            current.insert(set_id.clone());
                                                                        }
                                                                    })
                                                                }
                                                            />
                                                            <span class="form__checkbox-label">
                                                                {format!("{} ({} файлов)", set.label_ru, set.file_count)}
                                                            </span>
                                                        </label>
                                                    }
                                                }).collect_view()}
                                            </div>

                                            <div class="datasets__mode">
                                                <label class="form__checkbox-wrapper">
                                                    <input
                                                        type="radio"
                                                        name="restore-mode"
                                                        prop:checked=move || mode.get() == RestoreMode::Merge
                                                        on:change=move |_| mode.set(RestoreMode::Merge)
                                                    />
                                                    <span class="form__checkbox-label">
                                                        "Наложить поверх — лишние локальные файлы останутся"
                                                    </span>
                                                </label>
                                                <label class="form__checkbox-wrapper">
                                                    <input
                                                        type="radio"
                                                        name="restore-mode"
                                                        prop:checked=move || mode.get() == RestoreMode::Replace
                                                        on:change=move |_| mode.set(RestoreMode::Replace)
                                                    />
                                                    <span class="form__checkbox-label">
                                                        "Точная копия — лишние локальные файлы будут удалены"
                                                    </span>
                                                </label>
                                            </div>

                                            <button
                                                class="button button--primary"
                                                disabled=move || state.busy_label.get().is_some()
                                                on:click={
                                                    let id = snapshot_id.clone();
                                                    move |_| show_diff(id.clone())
                                                }
                                            >
                                                {icon("git-compare")}
                                                "Показать различия"
                                            </button>
                                        </div>
                                    </Show>
                                </div>
                            }
                        }).collect_view()}
                    </div>
                }.into_any()
            }}

            <DiffDialog state=state preview=preview />
        </section>
    }
}

/// Скачивание через blob: обычной ссылкой не обойтись — на роут нужен
/// заголовок Authorization.
fn trigger_download(filename: &str, bytes: &[u8]) -> Result<(), String> {
    use wasm_bindgen::JsCast;

    let array = js_sys::Uint8Array::from(bytes);
    let parts = js_sys::Array::new();
    parts.push(&array.buffer());
    let blob = web_sys::Blob::new_with_u8_array_sequence(&parts)
        .map_err(|_| "Не удалось подготовить файл".to_string())?;
    let url = web_sys::Url::create_object_url_with_blob(&blob)
        .map_err(|_| "Не удалось подготовить ссылку".to_string())?;

    let document = web_sys::window()
        .and_then(|window| window.document())
        .ok_or_else(|| "Нет доступа к документу".to_string())?;
    let anchor = document
        .create_element("a")
        .map_err(|_| "Не удалось создать ссылку".to_string())?
        .dyn_into::<web_sys::HtmlAnchorElement>()
        .map_err(|_| "Не удалось создать ссылку".to_string())?;
    anchor.set_href(&url);
    anchor.set_download(filename);
    anchor.click();
    let _ = web_sys::Url::revoke_object_url(&url);
    Ok(())
}
