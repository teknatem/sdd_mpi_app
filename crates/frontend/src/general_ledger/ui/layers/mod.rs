//! Каталог слоёв учёта GL (`general_ledger_layers`).
//!
//! По аналогии с реестрами измерений и оборотов: тянет `/api/general-ledger/layers`,
//! показывает каждый слой как цветной бейдж + имя + описание + количество проводок.

use crate::general_ledger::api::fetch_general_ledger_layers;
use crate::general_ledger::ui::layer_badge::GlLayerBadge;
use crate::shared::page_frame::PageFrame;
use crate::shared::page_standard::PAGE_CAT_LIST;
use contracts::general_ledger::GlLayerDto;
use leptos::prelude::*;
use leptos::task::spawn_local;
use thaw::*;

#[component]
pub fn GeneralLedgerLayersPage() -> impl IntoView {
    let (items, set_items) = signal(Vec::<GlLayerDto>::new());
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
            match fetch_general_ledger_layers().await {
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
        <PageFrame page_id="general_ledger_layers--list" category=PAGE_CAT_LIST>
            <div class="page__header">
                <div class="page__header-left">
                    <h1 class="page__title">"Слои GL"</h1>
                </div>
            </div>

            <div class="page__content">
                {move || {
                    if loading.get() {
                        return view! {
                            <div class="page__placeholder">
                                <Spinner /> " Загрузка слоёв..."
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
                                        <th class="table__header-cell" style="width: 90px;">"Слой"</th>
                                        <th class="table__header-cell" style="width: 18%;">"Наименование"</th>
                                        <th class="table__header-cell">"Описание"</th>
                                        <th class="table__header-cell" style="width: 12%;">"Проводок"</th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {rows.into_iter().map(|row| {
                                        view! {
                                            <tr class="table__row">
                                                <td class="table__cell">
                                                    <GlLayerBadge layer=row.code.clone() />
                                                </td>
                                                <td class="table__cell">{row.name.clone()}</td>
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
