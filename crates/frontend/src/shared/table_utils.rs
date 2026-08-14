//! Утилиты для таблиц: изменение ширины колонок с сохранением в localStorage.
//!
//! # Использование
//!
//! ```rust
//! use crate::shared::table_utils::init_column_resize;
//!
//! // В компоненте списка
//! Effect::new(move |_| {
//!     init_column_resize("my-table-id", "my_feature_column_widths");
//! });
//! ```
//!
//! В HTML таблицы:
//! ```html
//! <table id="my-table-id">
//!     <thead>
//!         <tr>
//!             <th class="resizable">Колонка 1</th>
//!             <th class="resizable">Колонка 2</th>
//!         </tr>
//!     </thead>
//! </table>
//! ```

use leptos::task::spawn_local;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{HtmlElement, MouseEvent as WebMouseEvent};

// Global registry: tracks which table IDs already have document-level (mousemove/mouseup)
// listeners attached. Prevents accumulation of closures when a component is unmounted and
// remounted — they are registered with `forget()` and cannot be removed later.
// Сами ручки (.table__resizer) под этот учёт не попадают: после перерисовки таблицы
// это уже другие <th>, и ручки нужно навесить заново.
thread_local! {
    static RESIZE_INITIALIZED: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
    static RESIZE_STATE: RefCell<HashMap<String, Rc<RefCell<ResizeState>>>> =
        RefCell::new(HashMap::new());
}

/// Колонка, которую тянут прямо сейчас. Состояние одно на таблицу — физически
/// нельзя тянуть две колонки сразу, а документные обработчики так переживают
/// перерисовку таблицы: они держат ссылку на состояние, а не на конкретный <th>.
#[derive(Default)]
struct ResizeState {
    th: Option<HtmlElement>,
    start_x: i32,
    start_width: i32,
    did_resize: bool,
}

fn resize_state(table_id: &str) -> Rc<RefCell<ResizeState>> {
    RESIZE_STATE.with(|map| {
        map.borrow_mut()
            .entry(table_id.to_string())
            .or_insert_with(|| Rc::new(RefCell::new(ResizeState::default())))
            .clone()
    })
}

/// Проверяет, было ли только что изменение ширины колонки.
/// Используется для блокировки клика сортировки сразу после resize.
pub fn was_just_resizing() -> bool {
    web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.body())
        .map(|b| b.get_attribute("data-was-resizing").as_deref() == Some("true"))
        .unwrap_or(false)
}

/// Очищает флаг resize.
pub fn clear_resize_flag() {
    if let Some(body) = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.body())
    {
        let _ = body.remove_attribute("data-was-resizing");
    }
}

/// Сохраняет ширины колонок в localStorage.
///
/// # Аргументы
/// * `table_id` - ID таблицы в DOM
/// * `storage_key` - Ключ для localStorage (должен быть уникальным для каждого списка)
pub fn save_column_widths(table_id: &str, storage_key: &str) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Some(document) = window.document() else {
        return;
    };
    let Some(storage) = window.local_storage().ok().flatten() else {
        return;
    };
    let Some(table) = document.get_element_by_id(table_id) else {
        return;
    };

    let headers = table.query_selector_all("th.resizable").ok();
    let Some(headers) = headers else { return };

    let mut widths: Vec<i32> = Vec::new();
    for i in 0..headers.length() {
        if let Some(th) = headers.get(i) {
            if let Ok(th) = th.dyn_into::<HtmlElement>() {
                widths.push(th.offset_width());
            }
        }
    }

    if let Ok(json) = serde_json::to_string(&widths) {
        let _ = storage.set_item(storage_key, &json);
    }
}

/// Восстанавливает ширины колонок из localStorage.
///
/// # Аргументы
/// * `table_id` - ID таблицы в DOM
/// * `storage_key` - Ключ для localStorage
pub fn restore_column_widths(table_id: &str, storage_key: &str) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Some(document) = window.document() else {
        return;
    };
    let Some(storage) = window.local_storage().ok().flatten() else {
        return;
    };
    let Some(table) = document.get_element_by_id(table_id) else {
        return;
    };

    let Some(json) = storage.get_item(storage_key).ok().flatten() else {
        return;
    };
    let Ok(widths): Result<Vec<i32>, _> = serde_json::from_str(&json) else {
        return;
    };

    let headers = table.query_selector_all("th.resizable").ok();
    let Some(headers) = headers else { return };

    for (i, width) in widths.iter().enumerate() {
        if let Some(th) = headers.get(i as u32) {
            if let Ok(th) = th.dyn_into::<HtmlElement>() {
                let _ = th.style().set_property("width", &format!("{}px", width));
                let _ = th
                    .style()
                    .set_property("min-width", &format!("{}px", width));
            }
        }
    }
}

/// Автоматически подбирает оптимальную ширину колонки по содержимому.
///
/// Использует простой подход: длина текста × 8px + padding.
/// Анализирует содержимое всех ячеек колонки на текущей странице.
///
/// # Аргументы
/// * `table_id` - ID таблицы в DOM
/// * `col_index` - Индекс колонки (0-based)
/// * `storage_key` - Ключ для localStorage
///
/// # Пример
/// ```rust
/// auto_fit_column("my-table-id", 2, "my_feature_column_widths");
/// ```
pub fn auto_fit_column(table_id: &str, col_index: u32, storage_key: &str) {
    // Early returns для упрощения структуры
    let Some(window) = web_sys::window() else {
        return;
    };
    let Some(document) = window.document() else {
        return;
    };
    let Some(table) = document.get_element_by_id(table_id) else {
        return;
    };

    // Найти заголовок колонки
    let Ok(headers) = table.query_selector_all("th.resizable") else {
        return;
    };
    let Some(th_element) = headers.get(col_index) else {
        return;
    };
    let Ok(th) = th_element.dyn_into::<HtmlElement>() else {
        return;
    };

    // Начальное значение - ширина заголовка
    let mut max_length = th.inner_text().len();

    // Проверить все строки tbody
    // Проверить все строки tbody
    if let Ok(Some(tbody)) = table.query_selector("tbody") {
        if let Ok(rows) = tbody.query_selector_all("tr") {
            for row_idx in 0..rows.length() {
                if let Some(row) = rows.get(row_idx) {
                    if let Some(element) = row.dyn_ref::<web_sys::Element>() {
                        if let Ok(cells) = element.query_selector_all("td") {
                            if let Some(cell) = cells.get(col_index) {
                                if let Ok(cell_html) = cell.dyn_into::<HtmlElement>() {
                                    let text_len = cell_html.inner_text().len();
                                    max_length = max_length.max(text_len);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    // Расчет ширины: 8px на символ + 40px padding
    // Минимум 60px, максимум 500px
    let optimal_width = ((max_length as i32 * 8) + 40).clamp(60, 500);

    // Установить ширину
    let _ = th
        .style()
        .set_property("width", &format!("{}px", optimal_width));
    let _ = th
        .style()
        .set_property("min-width", &format!("{}px", optimal_width));

    // Сохранить в localStorage
    save_column_widths(table_id, storage_key);
}

/// Инициализирует изменение ширины для всех колонок с классом "resizable".
///
/// Добавляет resize-handle к каждому заголовку и обрабатывает события мыши.
/// Ширины сохраняются в localStorage и восстанавливаются при следующем открытии.
///
/// Вызывать можно повторно: документные обработчики регистрируются один раз на
/// `table_id`, а ручки навешиваются на те `<th>`, у которых их ещё нет. Это нужно
/// после перерисовки таблицы — иначе на второй раз ресайз молча переставал работать.
///
/// # Аргументы
/// * `table_id` - ID таблицы в DOM
/// * `storage_key` - Ключ для localStorage (например, "a012_wb_sales_column_widths")
///
/// # Пример
/// ```rust
/// Effect::new(move |_| {
///     init_column_resize("wb-sales-table", "a012_wb_sales_column_widths");
/// });
/// ```
pub fn init_column_resize(table_id: &str, storage_key: &str) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Some(document) = window.document() else {
        return;
    };
    let Some(table) = document.get_element_by_id(table_id) else {
        return;
    };

    // First restore saved widths
    restore_column_widths(table_id, storage_key);

    let headers = table.query_selector_all("th.resizable").ok();
    let Some(headers) = headers else { return };

    let table_id_owned = table_id.to_string();
    let storage_key_owned = storage_key.to_string();
    let state = resize_state(table_id);

    // Документные обработчики — ровно один раз на таблицу за время жизни приложения.
    // insert() возвращает true только при первой регистрации.
    let first_registration =
        RESIZE_INITIALIZED.with(|set| set.borrow_mut().insert(table_id.to_string()));
    if first_registration {
        let state_mm = state.clone();
        let mousemove = Closure::wrap(Box::new(move |e: WebMouseEvent| {
            let mut state = state_mm.borrow_mut();
            let Some(th) = state.th.clone() else {
                return;
            };
            state.did_resize = true;
            let new_width = (state.start_width + e.client_x() - state.start_x).max(40);
            let _ = th.style().set_property("width", &format!("{}px", new_width));
            let _ = th.style().set_property("min-width", &format!("{}px", new_width));
        }) as Box<dyn FnMut(WebMouseEvent)>);

        let _ =
            document.add_event_listener_with_callback("mousemove", mousemove.as_ref().unchecked_ref());
        mousemove.forget();

        let state_mu = state.clone();
        let table_id_mu = table_id_owned.clone();
        let storage_key_mu = storage_key_owned.clone();
        let mouseup = Closure::wrap(Box::new(move |_: WebMouseEvent| {
            let was_resizing = {
                let mut state = state_mu.borrow_mut();
                if state.th.is_none() {
                    return;
                }
                state.th = None;
                std::mem::take(&mut state.did_resize)
            };

            if let Some(body) = web_sys::window()
                .and_then(|w| w.document())
                .and_then(|d| d.body())
            {
                let _ = body.class_list().remove_1("resizing-column");
                if was_resizing {
                    // Save column widths to localStorage
                    save_column_widths(&table_id_mu, &storage_key_mu);
                    let _ = body.set_attribute("data-was-resizing", "true");
                    spawn_local(async {
                        gloo_timers::future::TimeoutFuture::new(50).await;
                        clear_resize_flag();
                    });
                }
            }
        }) as Box<dyn FnMut(WebMouseEvent)>);

        let _ = document.add_event_listener_with_callback("mouseup", mouseup.as_ref().unchecked_ref());
        mouseup.forget();
    }

    for i in 0..headers.length() {
        let Some(th) = headers.get(i) else { continue };
        let Ok(th) = th.dyn_into::<HtmlElement>() else {
            continue;
        };

        // Skip if already has resize handle
        if th
            .query_selector(".table__resizer")
            .ok()
            .flatten()
            .is_some()
        {
            continue;
        }

        // Create resize handle
        let Ok(handle) = document.create_element("div") else {
            continue;
        };
        handle.set_class_name("table__resizer");

        // Double-click auto-fit handler
        let table_id_dblclick = table_id_owned.clone();
        let storage_key_dblclick = storage_key_owned.clone();
        let col_idx = i;

        let dblclick = Closure::wrap(Box::new(move |e: WebMouseEvent| {
            e.prevent_default();
            e.stop_propagation();
            auto_fit_column(&table_id_dblclick, col_idx, &storage_key_dblclick);
        }) as Box<dyn FnMut(WebMouseEvent)>);

        let _ =
            handle.add_event_listener_with_callback("dblclick", dblclick.as_ref().unchecked_ref());
        dblclick.forget();

        // Prevent click events from bubbling to prevent sorting trigger
        let click_blocker = Closure::wrap(Box::new(move |e: WebMouseEvent| {
            e.stop_propagation();
        }) as Box<dyn FnMut(WebMouseEvent)>);

        let _ = handle
            .add_event_listener_with_callback("click", click_blocker.as_ref().unchecked_ref());
        click_blocker.forget();

        // Mousedown на ручке: запоминаем тянущуюся колонку в состоянии таблицы,
        // дальше её ведут документные обработчики.
        let state_md = state.clone();
        let th_md = th.clone();

        let mousedown = Closure::wrap(Box::new(move |e: WebMouseEvent| {
            e.prevent_default();
            e.stop_propagation();
            {
                let mut state = state_md.borrow_mut();
                state.th = Some(th_md.clone());
                state.did_resize = false;
                state.start_x = e.client_x();
                state.start_width = th_md.offset_width();
            }

            if let Some(body) = web_sys::window()
                .and_then(|w| w.document())
                .and_then(|d| d.body())
            {
                let _ = body.class_list().add_1("resizing-column");
            }
        }) as Box<dyn FnMut(WebMouseEvent)>);

        let _ = handle
            .add_event_listener_with_callback("mousedown", mousedown.as_ref().unchecked_ref());
        mousedown.forget();

        let _ = th.append_child(&handle);
    }
}
