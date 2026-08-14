//! Каталог субъектов учёта GL (`general_ledger_entities`).
//!
//! По аналогии с каталогом слоёв: тянет `/api/general-ledger/entities`,
//! показывает каждый субъект как цветной бейдж + имя + вид + описание + кол-во
//! проводок. Субъект = контур (маркетплейс или собственная организация).

use crate::general_ledger::api::fetch_general_ledger_entities;
use crate::general_ledger::ui::entity_badge::GlEntityBadge;
use crate::shared::page_frame::PageFrame;
use crate::shared::page_standard::PAGE_CAT_LIST;
use contracts::general_ledger::GlEntityDto;
use leptos::prelude::*;
use leptos::task::spawn_local;
use thaw::*;

fn kind_label(kind: &str) -> &'static str {
    match kind {
        "marketplace" => "Маркетплейс",
        "own" => "Своя организация",
        _ => "—",
    }
}

#[component]
pub fn GeneralLedgerEntitiesPage() -> impl IntoView {
    let (items, set_items) = signal(Vec::<GlEntityDto>::new());
    let (loading, set_loading) = signal(false);
    let (error, set_error) = signal::<Option<String>>(None);
    let loaded = RwSignal::new(false);

    Effect::new(move |_| {
        if loaded.get_untracked() {
            return;
        }
        spawn_local(async move {
            set_loading.set(true);
            set_error.set(None);
            match fetch_general_ledger_entities().await {
                Ok(response) => {
                    set_items.set(response.items);
                    loaded.set(true);
                }
                Err(err) => set_error.set(Some(err)),
            }
            set_loading.set(false);
        });
    });

    view! {
        <PageFrame page_id="general_ledger_entities--list" category=PAGE_CAT_LIST>
            <div class="page__header">
                <div class="page__header-left">
                    <h1 class="page__title">"Субъекты учёта GL"</h1>
                </div>
            </div>

            <div class="page__content">
                {move || {
                    if loading.get() {
                        return view! {
                            <div class="page__placeholder">
                                <Spinner /> " Загрузка субъектов..."
                            </div>
                        }.into_any();
                    }
                    if let Some(err) = error.get() {
                        return view! {
                            <div class="alert alert--error">{format!("Ошибка: {err}")}</div>
                        }.into_any();
                    }

                    let rows = items.get();
                    view! {
                        <div class="table-wrap" style="width: 100%;">
                            <table class="table" style="width: 100%; table-layout: fixed;">
                                <thead>
                                    <tr class="table__header-row">
                                        <th class="table__header-cell" style="width: 90px;">"Субъект"</th>
                                        <th class="table__header-cell" style="width: 18%;">"Наименование"</th>
                                        <th class="table__header-cell" style="width: 14%;">"Вид"</th>
                                        <th class="table__header-cell">"Описание"</th>
                                        <th class="table__header-cell" style="width: 12%;">"Проводок"</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {rows.into_iter().map(|row| {
                                        view! {
                                            <tr class="table__row">
                                                <td class="table__cell">
                                                    <GlEntityBadge entity=row.code.clone() />
                                                </td>
                                                <td class="table__cell">{row.name.clone()}</td>
                                                <td class="table__cell">{kind_label(&row.kind)}</td>
                                                <td class="table__cell" style="overflow-wrap: anywhere; white-space: normal;">
                                                    {row.description.clone()}
                                                </td>
                                                <td class="table__cell table__cell--right">
                                                    {row.gl_entries_count}
                                                </td>
                                            </tr>
                                        }
                                    }).collect_view()}
                                </tbody>
                            </table>
                        </div>
                    }.into_any()
                }}
            </div>
        </PageFrame>
    }
}
