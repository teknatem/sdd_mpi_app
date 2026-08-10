//! Модалка различий: последняя точка, где ещё можно передумать.
//!
//! Применение стартует из этого же диалога и с тем же `bundle_sha256`, который
//! был посчитан для показанного диффа — иначе можно применить не то, что
//! подтвердил админ.

use contracts::system::datasets::{
    DiffClass, RestoreApplyRequest, RestoreMode, RestorePreviewDto, WarningSeverity,
};
use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::shared::date_utils::{format_bytes_compact, format_datetime_utc_local};
use crate::shared::modal_frame::ModalFrame;
use crate::system::datasets::api;
use crate::system::datasets::ui::{DatasetsState, DATETIME_FMT};

#[component]
pub fn DiffDialog(
    state: DatasetsState,
    preview: RwSignal<Option<RestorePreviewDto>>,
) -> impl IntoView {
    let class_filter = RwSignal::<Option<DiffClass>>::new(None);

    let close = Callback::new(move |_| {
        preview.set(None);
        class_filter.set(None);
    });

    let apply = move |dto: RestorePreviewDto| {
        let deleted: u64 = dto.sets.iter().map(|set| set.local_only).sum();
        if dto.mode == RestoreMode::Replace && deleted > 0 {
            let message = format!(
                "Режим «точная копия» удалит {deleted} локальных файлов, которых нет в снапшоте. \
                 Перед применением будет снят архив текущего состояния. Продолжить?"
            );
            let confirmed = web_sys::window()
                .and_then(|window| window.confirm_with_message(&message).ok())
                .unwrap_or(false);
            if !confirmed {
                return;
            }
        }

        let request = RestoreApplyRequest {
            snapshot_id: dto.snapshot_id.clone(),
            set_ids: dto.sets.iter().map(|set| set.set_id.clone()).collect(),
            mode: dto.mode,
            expected_bundle_sha256: Some(dto.bundle_sha256.clone()),
        };

        state.busy_label.set(Some("Восстановление данных...".to_string()));
        state.error.set(None);
        state.notice.set(None);
        spawn_local(async move {
            match api::restore_apply(&request).await {
                Ok(result) => {
                    let written: u64 = result.sets.iter().map(|set| set.files_written).sum();
                    let removed: u64 = result.sets.iter().map(|set| set.files_deleted).sum();
                    let archive = result
                        .pre_restore_archive
                        .clone()
                        .map(|path| format!(" Архив прежнего состояния: {path}"))
                        .unwrap_or_default();
                    state.notice.set(Some(format!(
                        "Восстановлено: записано {written} файлов, удалено {removed}.{archive}"
                    )));
                    preview.set(None);
                    state.reload.run(());
                }
                Err(err) => state.error.set(Some(err)),
            }
            state.busy_label.set(None);
        });
    };

    view! {
        <Show when=move || preview.get().is_some()>
            {move || {
                let Some(dto) = preview.get() else {
                    return ().into_any();
                };
                let dto_for_apply = dto.clone();
                let blocking = dto.blocking;

                view! {
                    <ModalFrame on_close=close modal_class="modal--wide".to_string()>
                        <div class="modal__header">
                            <h2 class="modal__title">"Различия перед восстановлением"</h2>
                            <button class="modal__close" on:click=move |_| close.run(())>"×"</button>
                        </div>

                        <div class="modal__body datasets__diff">
                            <div class="datasets__diff-source">
                                <div>
                                    <span class="text-muted">"Источник: "</span>
                                    <span class="datasets__instance-label">{dto.source.instance_label.clone()}</span>
                                    " · "
                                    <span class="datasets__mono">{dto.source.hostname.clone()}</span>
                                </div>
                                <div class="text-muted">
                                    {format!(
                                        "Снят {} · сборка {} ({}) · схема БД {}",
                                        format_datetime_utc_local(&dto.source.captured_at, DATETIME_FMT),
                                        dto.source.app_version,
                                        dto.source.git_commit,
                                        dto.source.schema_version
                                    )}
                                </div>
                                <div>
                                    <span class="text-muted">"Режим: "</span>
                                    {dto.mode.label_ru()}
                                </div>
                            </div>

                            {dto.warnings.iter().map(|warning| {
                                let class = match warning.severity {
                                    WarningSeverity::Blocking => "alert alert--error",
                                    WarningSeverity::Warning => "alert alert--warning",
                                    WarningSeverity::Info => "alert alert--info",
                                };
                                view! { <div class=class>{warning.message.clone()}</div> }
                            }).collect_view()}

                            <div class="table-wrapper">
                                <table class="table__data">
                                    <thead class="table__head">
                                        <tr>
                                            <th class="table__header-cell">"Набор"</th>
                                            <th class="table__header-cell datasets__col-num">"Добавится"</th>
                                            <th class="table__header-cell datasets__col-num">"Изменится"</th>
                                            <th class="table__header-cell datasets__col-num">"Только локально"</th>
                                            <th class="table__header-cell datasets__col-num">"Без изменений"</th>
                                            <th class="table__header-cell datasets__col-num">"Запишется"</th>
                                            <th class="table__header-cell datasets__col-num">"Удалится"</th>
                                        </tr>
                                    </thead>
                                    <tbody>
                                        {dto.sets.iter().map(|set| view! {
                                            <tr class="table__row">
                                                <td class="table__cell">
                                                    <div>{set.label_ru.clone()}</div>
                                                    <div class="datasets__mono text-muted">{set.local_path.clone()}</div>
                                                </td>
                                                <td class="table__cell datasets__col-num">{set.added.to_string()}</td>
                                                <td class="table__cell datasets__col-num">{set.modified.to_string()}</td>
                                                <td class="table__cell datasets__col-num">{set.local_only.to_string()}</td>
                                                <td class="table__cell datasets__col-num text-muted">
                                                    {set.identical.to_string()}
                                                </td>
                                                <td class="table__cell datasets__col-num">
                                                    {format_bytes_compact(set.bytes_written)}
                                                </td>
                                                <td class="table__cell datasets__col-num">
                                                    {if set.bytes_deleted > 0 {
                                                        format_bytes_compact(set.bytes_deleted)
                                                    } else {
                                                        "—".to_string()
                                                    }}
                                                </td>
                                            </tr>
                                        }).collect_view()}
                                    </tbody>
                                </table>
                            </div>

                            <div class="datasets__diff-filter">
                                <span class="text-muted">"Показать: "</span>
                                <FilterButton current=class_filter value=None label="все" />
                                <FilterButton current=class_filter value=Some(DiffClass::Modified) label="изменится" />
                                <FilterButton current=class_filter value=Some(DiffClass::Added) label="добавится" />
                                <FilterButton current=class_filter value=Some(DiffClass::LocalOnly) label="только локально" />
                                <FilterButton current=class_filter value=Some(DiffClass::Identical) label="без изменений" />
                            </div>

                            {dto.sets.iter().map(|set| {
                                let entries = set.entries.clone();
                                let truncated = set.entries_truncated;
                                let label = set.label_ru.clone();
                                view! {
                                    <div class="datasets__diff-set">
                                        <h3 class="datasets__diff-set-title">{label}</h3>
                                        <div class="table-wrapper datasets__diff-files">
                                            <table class="table__data table--striped">
                                                <tbody>
                                                    {move || {
                                                        let filter = class_filter.get();
                                                        entries.iter()
                                                            .filter(|entry| filter.is_none_or(|class| entry.class == class))
                                                            .map(|entry| view! {
                                                                <tr class="table__row">
                                                                    <td class="table__cell datasets__col-class">
                                                                        <span class=diff_class_badge(entry.class)>
                                                                            {entry.class.label_ru()}
                                                                        </span>
                                                                    </td>
                                                                    <td class="table__cell datasets__mono">{entry.path.clone()}</td>
                                                                    <td class="table__cell datasets__col-num text-muted">
                                                                        {size_column(entry.local_size_bytes, entry.bundle_size_bytes)}
                                                                    </td>
                                                                </tr>
                                                            })
                                                            .collect_view()
                                                    }}
                                                </tbody>
                                            </table>
                                        </div>
                                        {truncated.then(|| view! {
                                            <div class="text-muted">
                                                "Показаны первые записи — полный список есть в манифесте снапшота."
                                            </div>
                                        })}
                                    </div>
                                }
                            }).collect_view()}
                        </div>

                        <div class="modal__footer">
                            <button class="button button--secondary" on:click=move |_| close.run(())>
                                "Отмена"
                            </button>
                            <button
                                class="button button--primary"
                                disabled=move || blocking || state.busy_label.get().is_some()
                                title=if blocking { "Есть блокирующие предупреждения" } else { "" }
                                on:click={
                                    let dto = dto_for_apply.clone();
                                    move |_| apply(dto.clone())
                                }
                            >
                                "Восстановить"
                            </button>
                        </div>
                    </ModalFrame>
                }.into_any()
            }}
        </Show>
    }
}

#[component]
fn FilterButton(
    current: RwSignal<Option<DiffClass>>,
    value: Option<DiffClass>,
    label: &'static str,
) -> impl IntoView {
    view! {
        <button
            class=move || if current.get() == value {
                "datasets__filter datasets__filter--active"
            } else {
                "datasets__filter"
            }
            on:click=move |_| current.set(value)
        >
            {label}
        </button>
    }
}

fn diff_class_badge(class: DiffClass) -> &'static str {
    match class {
        DiffClass::Added => "badge badge--success",
        DiffClass::Modified => "badge badge--warning",
        DiffClass::LocalOnly => "badge badge--error",
        DiffClass::Identical => "badge badge--neutral",
    }
}

fn size_column(local: Option<u64>, bundle: Option<u64>) -> String {
    match (local, bundle) {
        (Some(local), Some(bundle)) if local != bundle => format!(
            "{} → {}",
            format_bytes_compact(local),
            format_bytes_compact(bundle)
        ),
        (_, Some(bundle)) => format_bytes_compact(bundle),
        (Some(local), None) => format_bytes_compact(local),
        (None, None) => "—".to_string(),
    }
}
