//! Вкладка «Журнал операций»: локальная история выгрузок и восстановлений.
//!
//! Журнал ведёт ЭТОТ экземпляр — что лежит в бакете, знает только каталог.
//! Поэтому строка здесь означает «мы это делали», а не «это существует».

use contracts::system::datasets::TransferLogEntryDto;
use leptos::prelude::*;

use crate::shared::date_utils::{format_bytes_compact, format_datetime_utc_local};
use crate::system::datasets::ui::DATETIME_FMT;

#[component]
pub fn JournalTab(history: RwSignal<Vec<TransferLogEntryDto>>) -> impl IntoView {
    view! {
        <section class="datasets__section">
            <h2 class="datasets__section-title">"Журнал операций"</h2>
            <p class="datasets__section-hint">
                "История операций этого экземпляра. Время местное (UTC+3)."
            </p>

            <Show
                when=move || !history.get().is_empty()
                fallback=|| view! {
                    <div class="text-muted">"Операций пока не было."</div>
                }
            >
                <div class="table-wrapper">
                    <table class="table__data table--striped">
                        <thead class="table__head">
                            <tr>
                                <th class="table__header-cell">"Когда"</th>
                                <th class="table__header-cell">"Операция"</th>
                                <th class="table__header-cell">"Снапшот"</th>
                                <th class="table__header-cell">"Источник"</th>
                                <th class="table__header-cell">"Наборы"</th>
                                <th class="table__header-cell">"Режим"</th>
                                <th class="table__header-cell datasets__col-num">"Файлов"</th>
                                <th class="table__header-cell datasets__col-num">"Объём"</th>
                                <th class="table__header-cell">"Итог"</th>
                            </tr>
                        </thead>
                        <tbody>
                            <For
                                each=move || history.get()
                                key=|entry| entry.id.clone()
                                let:entry
                            >
                                <tr class="table__row">
                                    <td class="table__cell datasets__col-when">
                                        {format_datetime_utc_local(&entry.created_at, DATETIME_FMT)}
                                    </td>
                                    <td class="table__cell">
                                        {if entry.operation == "snapshot" { "Выгрузка" } else { "Восстановление" }}
                                    </td>
                                    <td class="table__cell datasets__mono">{entry.snapshot_id.clone()}</td>
                                    <td class="table__cell datasets__mono">
                                        {entry.source_instance_id.clone().unwrap_or_else(|| "—".to_string())}
                                    </td>
                                    <td class="table__cell">{entry.set_ids.join(", ")}</td>
                                    <td class="table__cell">
                                        {entry.mode.clone().unwrap_or_else(|| "—".to_string())}
                                    </td>
                                    <td class="table__cell datasets__col-num">
                                        {files_column(entry.files_written, entry.files_deleted)}
                                    </td>
                                    <td class="table__cell datasets__col-num">
                                        {format_bytes_compact(entry.bytes.max(0) as u64)}
                                    </td>
                                    <td class="table__cell">
                                        {
                                            let (class, label) = match entry.status.as_str() {
                                                "ok" => ("badge badge--success", "Успешно"),
                                                "rolled_back" => ("badge badge--warning", "Откат"),
                                                _ => ("badge badge--error", "Ошибка"),
                                            };
                                            let error = entry.error.clone();
                                            view! {
                                                <span class=class title=error.unwrap_or_default()>{label}</span>
                                            }
                                        }
                                    </td>
                                </tr>
                            </For>
                        </tbody>
                    </table>
                </div>
            </Show>
        </section>
    }
}

/// «Записано / удалено» одной ячейкой: удаление показываем только когда оно было.
fn files_column(written: i64, deleted: i64) -> String {
    if deleted > 0 {
        format!("{written} / −{deleted}")
    } else {
        written.to_string()
    }
}
