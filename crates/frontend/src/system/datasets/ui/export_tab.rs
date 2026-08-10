//! Вкладка «Выгрузка»: выбор наборов и запись снапшота в S3.

use std::collections::HashSet;

use contracts::system::datasets::{CreateSnapshotRequest, CreateSnapshotResponse, PathOrigin};
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::shared::date_utils::format_bytes_compact;
use crate::shared::icons::icon;
use crate::system::datasets::api;
use crate::system::datasets::ui::DatasetsState;

#[component]
pub fn ExportTab(state: DatasetsState) -> impl IntoView {
    let selected = RwSignal::<HashSet<String>>::new(HashSet::new());
    let note = RwSignal::new(String::new());
    let result = RwSignal::<Option<CreateSnapshotResponse>>::new(None);
    let initialised = RwSignal::new(false);

    // Первичный выбор — по умолчанию из реестра; после этого не трогаем, иначе
    // обновление состояния сбрасывало бы галочки пользователя.
    Effect::new(move |_| {
        if initialised.get() {
            return;
        }
        if let Some(status) = state.status.get() {
            let defaults: HashSet<String> = status
                .sets
                .iter()
                .filter(|set| set.available && set.selected_by_default)
                .map(|set| set.set_id.clone())
                .collect();
            selected.set(defaults);
            initialised.set(true);
        }
    });

    let toggle = move |set_id: String| {
        selected.update(|current| {
            if !current.remove(&set_id) {
                current.insert(set_id);
            }
        });
    };

    let selected_summary = Signal::derive(move || {
        let chosen = selected.get();
        let Some(status) = state.status.get() else {
            return (0_u64, 0_u64);
        };
        status
            .sets
            .iter()
            .filter(|set| chosen.contains(&set.set_id))
            .fold((0, 0), |(files, bytes), set| {
                (files + set.file_count, bytes + set.total_bytes)
            })
    });

    let write_snapshot = move |_| {
        let set_ids: Vec<String> = selected.get().into_iter().collect();
        if set_ids.is_empty() {
            state.error.set(Some("Не выбрано ни одного набора".to_string()));
            return;
        }
        let note_value = note.get().trim().to_string();
        let request = CreateSnapshotRequest {
            set_ids,
            note: (!note_value.is_empty()).then_some(note_value),
        };

        state.busy_label.set(Some("Запись снапшота в S3...".to_string()));
        state.error.set(None);
        state.notice.set(None);
        result.set(None);
        spawn_local(async move {
            match api::create_snapshot(&request).await {
                Ok(response) => {
                    state.notice.set(Some(format!(
                        "Снапшот {} записан ({})",
                        response.snapshot.snapshot_id,
                        format_bytes_compact(response.bundle_size_bytes)
                    )));
                    result.set(Some(response));
                    state.reload.run(());
                }
                Err(err) => state.error.set(Some(err)),
            }
            state.busy_label.set(None);
        });
    };

    view! {
        <section class="datasets__section">
            <h2 class="datasets__section-title">"Что записать"</h2>
            <p class="datasets__section-hint">
                "Столбец «Происхождение пути» показывает, откуда взят каталог: общий корень данных, \
                 исторический абсолютный путь в своей секции конфига или путь относительно бинарника."
            </p>

            <div class="table-wrapper">
                <table class="table__data">
                    <thead class="table__head">
                        <tr>
                            <th class="table__header-cell datasets__col-check"></th>
                            <th class="table__header-cell">"Набор"</th>
                            <th class="table__header-cell">"Путь"</th>
                            <th class="table__header-cell">"Происхождение пути"</th>
                            <th class="table__header-cell datasets__col-num">"Файлов"</th>
                            <th class="table__header-cell datasets__col-num">"Размер"</th>
                        </tr>
                    </thead>
                    <tbody>
                        {move || state.status.get().map(|status| {
                            status.sets.into_iter().map(|set| {
                                let set_id = set.set_id.clone();
                                let checkbox_id = set_id.clone();
                                let disabled = !set.available;
                                let title = set.unavailable_reason.clone().unwrap_or_else(|| set.description_ru.clone());
                                view! {
                                    <tr class="table__row" class:datasets__row--disabled=disabled>
                                        <td class="table__cell datasets__col-check">
                                            <input
                                                type="checkbox"
                                                class="form__checkbox"
                                                prop:disabled=disabled
                                                prop:checked=move || selected.get().contains(&checkbox_id)
                                                on:change={
                                                    let set_id = set_id.clone();
                                                    move |_| toggle(set_id.clone())
                                                }
                                            />
                                        </td>
                                        <td class="table__cell">
                                            <div class="datasets__set-name" title=title.clone()>
                                                {set.label_ru.clone()}
                                                {(!set.exists).then(|| view! {
                                                    <span class="badge badge--neutral">"нет каталога"</span>
                                                })}
                                                {disabled.then(|| view! {
                                                    <span class="badge badge--neutral">"фаза 2"</span>
                                                })}
                                            </div>
                                            <div class="datasets__set-description">{set.description_ru.clone()}</div>
                                        </td>
                                        <td class="table__cell datasets__mono">{set.path.clone()}</td>
                                        <td class="table__cell">
                                            <span class=origin_class(set.path_origin)>
                                                {set.path_origin.label_ru()}
                                            </span>
                                        </td>
                                        <td class="table__cell datasets__col-num">{set.file_count.to_string()}</td>
                                        <td class="table__cell datasets__col-num">
                                            {format_bytes_compact(set.total_bytes)}
                                        </td>
                                    </tr>
                                }
                            }).collect_view()
                        })}
                    </tbody>
                </table>
            </div>

            <div class="datasets__export-controls">
                <label class="form__field datasets__note">
                    <span class="form__label">"Комментарий к снапшоту"</span>
                    <input
                        type="text"
                        class="form__input"
                        placeholder="Например: перед обновлением навыков"
                        prop:value=move || note.get()
                        on:input=move |ev| note.set(event_target_value(&ev))
                    />
                </label>
                <div class="datasets__export-summary">
                    {move || {
                        let (files, bytes) = selected_summary.get();
                        format!("Выбрано: {} файлов, {}", files, format_bytes_compact(bytes))
                    }}
                </div>
                <button
                    class="button button--primary"
                    disabled=move || state.busy_label.get().is_some() || selected.get().is_empty()
                    on:click=write_snapshot
                >
                    {icon("upload-cloud")}
                    "Записать в S3"
                </button>
            </div>

            {move || result.get().map(|response| {
                let snapshot_id = response.snapshot.snapshot_id.clone();
                view! {
                    <div class="datasets__result">
                        <div class="datasets__result-row">
                            <span class="text-muted">"Снапшот"</span>
                            <span class="datasets__mono">{snapshot_id.clone()}</span>
                        </div>
                        <div class="datasets__result-row">
                            <span class="text-muted">"Ключ в бакете"</span>
                            <span class="datasets__mono">{response.bundle_key.clone()}</span>
                        </div>
                        <div class="datasets__result-row">
                            <span class="text-muted">"Размер / sha256"</span>
                            <span class="datasets__mono">
                                {format!("{} · {}", format_bytes_compact(response.bundle_size_bytes), response.bundle_sha256)}
                            </span>
                        </div>
                        {(!response.rotated_away.is_empty()).then(|| view! {
                            <div class="datasets__result-row">
                                <span class="text-muted">"Удалено ротацией"</span>
                                <span class="datasets__mono">{response.rotated_away.join(", ")}</span>
                            </div>
                        })}
                        {response.warnings.iter().map(|warning| view! {
                            <div class="alert alert--warning">{warning.clone()}</div>
                        }).collect_view()}
                    </div>
                }
            })}
        </section>
    }
}

fn origin_class(origin: PathOrigin) -> &'static str {
    match origin {
        PathOrigin::DataRoot => "badge badge--success",
        PathOrigin::LegacyOverride => "badge badge--warning",
        PathOrigin::ExeRelative => "badge badge--neutral",
    }
}
