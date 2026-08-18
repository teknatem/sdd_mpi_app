//! Stickers tab — select rows, then batch-download from WB API

use super::super::api::{fetch_stickers_for_ids, NomenclatureInfo, StickerDto, SupplyOrderDto};
use super::super::view_model::WbSupplyDetailsVm;
use crate::layout::global_context::AppGlobalContext;
use leptos::prelude::*;
use std::collections::{HashMap, HashSet};
use thaw::*;
use wasm_bindgen::prelude::*;
use web_sys::{window, BlobPropertyBag, HtmlAnchorElement, Url};

// ---------------------------------------------------------------------------
// Download / print helpers
// ---------------------------------------------------------------------------

fn ext_for(t: &str) -> &'static str {
    match t {
        "svg" => "svg",
        "zplv" | "zplh" => "zpl",
        _ => "png",
    }
}

/// Format the WB sticker number "partA-partB". The second part always has a
/// fixed width of four digits, padded with leading zeros
/// (e.g. 5409058-284 → 5409058-0284).
fn format_sticker_number(part_a: i64, part_b: i64) -> String {
    format!("{}-{:04}", part_a, part_b)
}

/// Trigger a file download via a temporary <a> element.
fn trigger_anchor_download(href: &str, filename: &str) {
    let Some(win) = window() else { return };
    let Some(doc) = win.document() else { return };
    let Ok(el) = doc.create_element("a") else {
        return;
    };
    let anchor: HtmlAnchorElement = el.unchecked_into();
    anchor.set_href(href);
    anchor.set_download(filename);
    let _ = doc.body().map(|b| b.append_child(&anchor));
    anchor.click();
    let _ = doc.body().map(|b| b.remove_child(&anchor));
}

/// Download a base64-encoded sticker file.
/// For binary formats (PNG/SVG): uses data-URL directly.
/// For text formats (ZPL): decodes base64 via browser atob → Blob URL, which avoids
/// Chrome's block on downloading `data:text/plain;base64,...` URLs.
fn download_sticker(s: &StickerDto, stype: &str) {
    let Some(ref b64) = s.file else { return };
    let fname = match (s.part_a, s.part_b) {
        (Some(a), Some(b)) => format!("sticker_{}.{}", format_sticker_number(a, b), ext_for(stype)),
        (Some(a), None) => format!("sticker_{}.{}", a, ext_for(stype)),
        _ => format!("sticker_{}.{}", s.order_id, ext_for(stype)),
    };

    let is_zpl = stype == "zplv" || stype == "zplh";

    if is_zpl {
        // Decode base64 → binary string → Uint8Array → Blob → Blob URL
        let Some(win) = window() else { return };
        let Ok(binary_str) = win.atob(b64) else {
            // Not base64 — treat as raw text
            let arr = js_sys::Array::new();
            arr.push(&JsValue::from_str(b64));
            let opts = BlobPropertyBag::new();
            opts.set_type("application/octet-stream");
            if let Ok(blob) = web_sys::Blob::new_with_str_sequence_and_options(&arr, &opts) {
                if let Ok(url) = Url::create_object_url_with_blob(&blob) {
                    trigger_anchor_download(&url, &fname);
                    let _ = Url::revoke_object_url(&url);
                }
            }
            return;
        };

        // Convert binary string to Uint8Array byte-by-byte
        let len = binary_str.len() as u32;
        let uint8 = js_sys::Uint8Array::new_with_length(len);
        for (i, ch) in binary_str.chars().enumerate() {
            uint8.set_index(i as u32, ch as u8);
        }
        let buf: JsValue = uint8.buffer().into();
        let parts = js_sys::Array::new();
        parts.push(&buf);
        let opts = BlobPropertyBag::new();
        opts.set_type("application/octet-stream");
        if let Ok(blob) = web_sys::Blob::new_with_buffer_source_sequence_and_options(&parts, &opts)
        {
            if let Ok(url) = Url::create_object_url_with_blob(&blob) {
                trigger_anchor_download(&url, &fname);
                let _ = Url::revoke_object_url(&url);
            }
        }
    } else {
        // PNG / SVG — data URL works fine in all browsers
        let mime = match stype {
            "svg" => "image/svg+xml",
            _ => "image/png",
        };
        trigger_anchor_download(&format!("data:{};base64,{}", mime, b64), &fname);
    }
}

fn open_print_window(stickers: &[StickerDto], supply_id: &str, stype: &str) {
    let Some(win) = window() else { return };
    let is_img = stype == "png" || stype == "svg";
    let mime = match stype {
        "svg" => "image/svg+xml",
        _ => "image/png",
    };

    let mut html = format!(
        r#"<!DOCTYPE html><html><head><meta charset="utf-8">
<title>Стикеры {sid}</title>
<style>
body{{margin:0;padding:6px;font-family:Arial,sans-serif;font-size:8pt;}}
.wrap{{display:flex;flex-wrap:wrap;gap:3mm;}}
.card{{border:0.5px solid #bbb;padding:2mm;text-align:center;page-break-inside:avoid;min-width:60mm;}}
.card img{{max-width:58mm;display:block;margin:0 auto;}}
.lbl{{font-weight:bold;font-size:10pt;margin-bottom:1mm;}}
@media print{{body{{padding:0;}}}}
</style></head><body>
<div style="font-weight:bold;margin-bottom:4mm;">Стикеры {sid}</div>
<div class="wrap">"#,
        sid = supply_id,
    );

    for s in stickers {
        let lbl = match (s.part_a, s.part_b) {
            (Some(a), Some(b)) => format_sticker_number(a, b),
            (Some(a), None) => a.to_string(),
            _ => s.order_id.to_string(),
        };
        let art = s.article.as_deref().unwrap_or("");
        html.push_str(&format!(
            "<div class='card'><div class='lbl'>{}</div><div style='font-size:7pt;color:#666;'>{}</div>",
            lbl, art
        ));
        if let Some(ref f) = s.file {
            if is_img {
                html.push_str(&format!(
                    "<img src='data:{};base64,{}' alt='{}'>",
                    mime, f, lbl
                ));
            }
        }
        html.push_str("</div>");
    }

    html.push_str("</div><script>window.onload=()=>window.print();</script></body></html>");

    let arr = js_sys::Array::new();
    arr.push(&JsValue::from_str(&html));
    let opts = BlobPropertyBag::new();
    opts.set_type("text/html;charset=utf-8");
    if let Ok(blob) = web_sys::Blob::new_with_str_sequence_and_options(&arr, &opts) {
        if let Ok(url) = Url::create_object_url_with_blob(&blob) {
            let _ = win.open_with_url_and_target(&url, "_blank");
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers for order rows
// ---------------------------------------------------------------------------

fn sticker_ab(part_a: Option<i64>, part_b: Option<i64>) -> String {
    match (part_a, part_b) {
        (Some(a), Some(b)) => format_sticker_number(a, b),
        (Some(a), None) => a.to_string(),
        _ => "—".to_string(),
    }
}

fn order_sticker_ab(o: &SupplyOrderDto) -> String {
    sticker_ab(o.part_a, o.part_b)
}

fn order_barcode(o: &SupplyOrderDto) -> String {
    // shkid (Statistics API sticker field) → first barcode → em-dash
    o.color_code
        .clone()
        .or_else(|| o.barcodes.first().cloned())
        .unwrap_or_else(|| "—".to_string())
}

const ZERO_UUID: &str = "00000000-0000-0000-0000-000000000000";

fn effective_base_nomenclature_ref(order: &SupplyOrderDto) -> Option<String> {
    order
        .base_nomenclature_ref
        .as_deref()
        .map(str::trim)
        .filter(|base_ref| {
            !base_ref.is_empty()
                && *base_ref != ZERO_UUID
                && Some(*base_ref) != order.nomenclature_ref.as_deref()
        })
        .map(ToOwned::to_owned)
}

fn nomenclature_label(info_map: &HashMap<String, NomenclatureInfo>, nom_ref: &str) -> String {
    info_map
        .get(nom_ref)
        .map(|info| {
            if info.article.trim().is_empty() {
                info.description.clone()
            } else {
                format!("{} (арт. {})", info.description, info.article)
            }
        })
        .unwrap_or_else(|| nom_ref.to_string())
}

fn nomenclature_link(
    tabs_store: AppGlobalContext,
    nomenclatures_info: RwSignal<HashMap<String, NomenclatureInfo>>,
    nom_ref: Option<String>,
    tab_title: &'static str,
) -> impl IntoView {
    if let Some(nom_ref_value) = nom_ref {
        let nom_ref_for_click = nom_ref_value.clone();
        let title = nom_ref_value.clone();
        view! {
            <a
                href="#"
                title=title
                on:click=move |ev: web_sys::MouseEvent| {
                    ev.prevent_default();
                    tabs_store.open_tab(
                        &format!("a004_nomenclature_details_{}", nom_ref_for_click),
                        tab_title,
                    );
                }
                style="color: #0078d4; text-decoration: underline; cursor: pointer;"
            >
                {move || nomenclature_label(&nomenclatures_info.get(), &nom_ref_value)}
            </a>
        }
        .into_any()
    } else {
        view! { <span>"—"</span> }.into_any()
    }
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

#[component]
pub fn StickersTab(vm: WbSupplyDetailsVm) -> impl IntoView {
    let supply_sig = vm.supply;
    let sticker_type = vm.sticker_type;
    let id_sig = vm.id;
    let tabs_store =
        leptos::context::use_context::<AppGlobalContext>().expect("AppGlobalContext not found");
    let nomenclatures_info = vm.nomenclatures_info;

    // Component-local state
    let checked: RwSignal<HashSet<i64>> = RwSignal::new(HashSet::new());
    let downloading = RwSignal::new(false);
    let download_error: RwSignal<Option<String>> = RwSignal::new(None);
    let last_stickers: RwSignal<Vec<StickerDto>> = RwSignal::new(vec![]);
    let show_print = RwSignal::new(false);

    // Orders from the loaded supply
    let orders = Signal::derive(move || {
        supply_sig
            .get()
            .map(|s| s.supply_orders)
            .unwrap_or_default()
    });

    // Selectable = have numeric WB order ID
    let selectable_ids = Signal::derive(move || {
        orders
            .get()
            .iter()
            .filter_map(|o| {
                if o.order_id > 0 {
                    Some(o.order_id)
                } else {
                    None
                }
            })
            .collect::<HashSet<i64>>()
    });

    let all_selected = Signal::derive(move || {
        let sel = selectable_ids.get();
        if sel.is_empty() {
            return false;
        }
        let chk = checked.get();
        sel.iter().all(|id| chk.contains(id))
    });

    let selected_count = Signal::derive(move || checked.get().len());

    let supply_id_str = Signal::derive(move || {
        supply_sig
            .get()
            .map(|s| s.header.supply_id.clone())
            .unwrap_or_default()
    });

    let select_all = move |_| {
        let ids = selectable_ids.get();
        checked.update(|c| {
            for id in ids {
                c.insert(id);
            }
        });
    };

    let deselect_all = move |_| {
        checked.update(|c| c.clear());
    };

    // Download selected
    let do_download = move || {
        let ids: Vec<i64> = checked.get().into_iter().collect();
        if ids.is_empty() {
            return;
        }
        let Some(supply_uuid) = id_sig.get() else {
            return;
        };
        let stype = sticker_type.get();
        downloading.set(true);
        download_error.set(None);
        show_print.set(false);

        leptos::task::spawn_local(async move {
            match fetch_stickers_for_ids(&supply_uuid, &ids, &stype).await {
                Ok(resp) => {
                    if let Some(w) = resp.warning {
                        download_error.set(Some(w));
                    }
                    let fetched = resp.stickers;
                    // Trigger download for each sticker that has a file
                    let mut downloaded = 0usize;
                    for s in &fetched {
                        if s.file.is_some() {
                            download_sticker(s, &stype);
                            downloaded += 1;
                        }
                    }
                    if downloaded == 0 && download_error.get().is_none() {
                        download_error.set(Some(
                            "Файлы стикеров не получены. Убедитесь что заказы загружены через «Оперативные заказы».".into()
                        ));
                    }
                    last_stickers.set(fetched);
                    show_print.set(downloaded > 0);
                }
                Err(e) => {
                    download_error.set(Some(e));
                }
            }
            downloading.set(false);
        });
    };

    view! {
        <div style="width: 100%; min-width: 0;">

            // Header
            <div style="display:flex; justify-content:space-between; align-items:flex-start; margin-bottom:12px; gap:12px; flex-wrap:wrap;">
                <h3 class="details-section__title" style="margin:0;">"Стикеры WB"</h3>

                <div style="display:flex; align-items:center; gap:8px; flex-wrap:wrap;">
                    // Format selector
                    <select
                        class="doc-filter__select"
                        style="font-size:var(--font-size-sm);"
                        on:change=move |ev| {
                            use wasm_bindgen::JsCast;
                            if let Some(el) = ev.target()
                                .and_then(|t| t.dyn_into::<web_sys::HtmlSelectElement>().ok())
                            {
                                sticker_type.set(el.value());
                            }
                        }
                    >
                        <option value="png">"PNG (58×40 мм)"</option>
                        <option value="svg">"SVG (масштабируемый)"</option>
                        <option value="zplv" selected=true>"ZPL вертикальный"</option>
                        <option value="zplh">"ZPL горизонтальный"</option>
                    </select>

                    // Select all / Deselect all
                    <Button
                        appearance=ButtonAppearance::Secondary
                        size=ButtonSize::Small
                        on_click=select_all
                        disabled=Signal::derive(move || selectable_ids.get().is_empty())
                    >
                        "☑ Выбрать все"
                    </Button>
                    <Button
                        appearance=ButtonAppearance::Secondary
                        size=ButtonSize::Small
                        on_click=deselect_all
                        disabled=Signal::derive(move || checked.get().is_empty())
                    >
                        "☐ Снять все"
                    </Button>

                    // Download selected
                    <Button
                        appearance=ButtonAppearance::Primary
                        size=ButtonSize::Small
                        on_click=move |_| do_download()
                        disabled=Signal::derive(move || downloading.get() || checked.get().is_empty())
                    >
                        {move || {
                            let n = selected_count.get();
                            if downloading.get() {
                                "Загрузка...".to_string()
                            } else if n > 0 {
                                format!("⬇ Скачать выбранные ({})", n)
                            } else {
                                "⬇ Скачать выбранные".to_string()
                            }
                        }}
                    </Button>

                    // Print PDF — appears after successful download
                    {move || {
                        if !show_print.get() {
                            return view! { <></> }.into_any();
                        }
                        let stickers = last_stickers.get();
                        let sid = supply_id_str.get();
                        let stype = sticker_type.get();
                        view! {
                            <Button
                                appearance=ButtonAppearance::Secondary
                                size=ButtonSize::Small
                                on_click=move |_| open_print_window(&stickers, &sid, &stype)
                            >
                                "🖨 Печать / PDF"
                            </Button>
                        }.into_any()
                    }}
                </div>
            </div>

            // Error / warning
            {move || download_error.get().map(|err| view! {
                <div style="color:var(--color-warning, #b45309); padding:8px 10px; border-radius:var(--radius-sm); background:var(--color-warning-bg, #fffbeb); border-left:3px solid var(--color-warning, #b45309); margin-bottom:10px; font-size:var(--font-size-sm); white-space:pre-wrap;">
                    {err}
                </div>
            })}

            // Loading indicator
            {move || downloading.get().then(|| view! {
                <div style="padding:6px 0; color:var(--color-text-secondary); font-size:var(--font-size-sm);">
                    "Загрузка стикеров из WB..."
                </div>
            })}

            // Table
            {move || {
                let rows = orders.get();
                let total = rows.len();
                let with_id = rows.iter().filter(|o| o.order_id > 0).count();

                if rows.is_empty() {
                    return view! {
                        <div style="padding:24px 0; color:var(--color-text-secondary); font-size:var(--font-size-sm);">
                            "Заказы не найдены. Загрузите поставки и заказы."
                        </div>
                    }.into_any();
                }

                view! {
                    <div>
                        // Status line
                        <div style="font-size:var(--font-size-sm); color:var(--color-text-secondary); margin-bottom:8px;">
                            {format!("Всего: {} заказов · {} с WB numeric ID (доступны для скачивания)", total, with_id)}
                            {if with_id < total {
                                view! {
                                    <span style="color:var(--color-warning, #b45309); margin-left:8px;">
                                        "· Для остальных запустите «Оперативные заказы»"
                                    </span>
                                }.into_any()
                            } else {
                                view! { <></> }.into_any()
                            }}
                        </div>

                        <div class="table-wrapper" style="width: 100%; min-width: 0; overflow-x: auto;">
                            <Table attr:style="width: 100%; min-width: 970px;">
                                <TableHeader>
                                    <TableRow>
                                        <TableHeaderCell resizable=false min_width=40.0 class="fixed-checkbox-column">
                                            // Header checkbox — toggles all selectable
                                            <input
                                                type="checkbox"
                                                style="cursor:pointer; width:16px; height:16px;"
                                                prop:checked=move || all_selected.get()
                                                on:change=move |_| {
                                                    if all_selected.get() {
                                                        checked.update(|c| c.clear());
                                                    } else {
                                                        let ids = selectable_ids.get();
                                                        checked.update(|c| {
                                                            for id in ids { c.insert(id); }
                                                        });
                                                    }
                                                }
                                            />
                                        </TableHeaderCell>
                                        <TableHeaderCell resizable=false min_width=40.0 class="width-40px">"#"</TableHeaderCell>
                                        <TableHeaderCell resizable=true min_width=140.0>"Артикул"</TableHeaderCell>
                                        <TableHeaderCell resizable=true min_width=110.0>"Стикер A-B"</TableHeaderCell>
                                        <TableHeaderCell resizable=true min_width=130.0>"Баркод"</TableHeaderCell>
                                        <TableHeaderCell resizable=true min_width=220.0>"Номенклатура 1С"</TableHeaderCell>
                                        <TableHeaderCell resizable=true min_width=220.0>"Базовая номенклатура"</TableHeaderCell>
                                        <TableHeaderCell resizable=true min_width=70.0>"WB ID"</TableHeaderCell>
                                    </TableRow>
                                </TableHeader>
                                <TableBody>
                                    {rows.into_iter().enumerate().map(|(i, order)| {
                                        let oid = order.order_id;
                                        let has_id = oid > 0;
                                        let article = order.article.clone()
                                            .unwrap_or_else(|| "—".to_string());
                                        let ab = order_sticker_ab(&order);
                                        let barcode = order_barcode(&order);
                                        let nomenclature_ref = order.nomenclature_ref.clone();
                                        let base_nomenclature_ref = effective_base_nomenclature_ref(&order);

                                        let row_style = if has_id {
                                            ""
                                        } else {
                                            "opacity:0.55;"
                                        };

                                        view! {
                                            <TableRow>
                                                <TableCell class="fixed-checkbox-column">
                                                    <TableCellLayout>
                                                        <input
                                                            type="checkbox"
                                                            style="cursor:pointer; width:16px; height:16px;"
                                                            disabled=!has_id
                                                            prop:checked=move || checked.get().contains(&oid)
                                                            on:change=move |_| {
                                                                if !has_id { return; }
                                                                checked.update(|c| {
                                                                    if c.contains(&oid) {
                                                                        c.remove(&oid);
                                                                    } else {
                                                                        c.insert(oid);
                                                                    }
                                                                });
                                                            }
                                                        />
                                                    </TableCellLayout>
                                                </TableCell>

                                                <TableCell class="width-40px"><TableCellLayout>
                                                    <span style=format!("color:var(--color-text-secondary); font-size:0.8em; {}", row_style)>
                                                        {i + 1}
                                                    </span>
                                                </TableCellLayout></TableCell>

                                                <TableCell><TableCellLayout truncate=true>
                                                    <strong style=row_style>{article}</strong>
                                                </TableCellLayout></TableCell>

                                                <TableCell><TableCellLayout>
                                                    <span style=format!("color:var(--color-accent); {}", row_style)>
                                                        {ab}
                                                    </span>
                                                </TableCellLayout></TableCell>

                                                <TableCell><TableCellLayout truncate=true>
                                                    <code style=format!("font-size:0.8em; {}", row_style)>
                                                        {barcode}
                                                    </code>
                                                </TableCellLayout></TableCell>

                                                <TableCell><TableCellLayout truncate=true>
                                                    {nomenclature_link(
                                                        tabs_store.clone(),
                                                        nomenclatures_info,
                                                        nomenclature_ref,
                                                        "Номенклатура",
                                                    )}
                                                </TableCellLayout></TableCell>

                                                <TableCell><TableCellLayout truncate=true>
                                                    {nomenclature_link(
                                                        tabs_store.clone(),
                                                        nomenclatures_info,
                                                        base_nomenclature_ref,
                                                        "Базовая номенклатура",
                                                    )}
                                                </TableCellLayout></TableCell>

                                                <TableCell><TableCellLayout>
                                                    {if has_id {
                                                        view! {
                                                            <code style="font-size:0.75em; color:var(--color-text-secondary);">
                                                                {oid.to_string()}
                                                            </code>
                                                        }.into_any()
                                                    } else {
                                                        view! {
                                                            <span style="font-size:0.75em; color:var(--color-text-secondary);">
                                                                "нет ID"
                                                            </span>
                                                        }.into_any()
                                                    }}
                                                </TableCellLayout></TableCell>
                                            </TableRow>
                                        }
                                    }).collect::<Vec<_>>()}
                                </TableBody>
                            </Table>
                        </div>

                        <div style="margin-top:8px; font-size:var(--font-size-xs); color:var(--color-text-secondary);">
                            "PDF: после скачивания нажмите «Печать / PDF» → в браузере «Сохранить как PDF»."
                        </div>
                    </div>
                }.into_any()
            }}

        </div>
    }
}
