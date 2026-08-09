use super::super::details::NomenclatureDetails;
use crate::shared::api_utils::api_base;
use crate::shared::icons::icon;
use crate::shared::modal_stack::ModalStackService;
use contracts::domain::a004_nomenclature::aggregate::Nomenclature;
use contracts::domain::common::{AggregateId, AggregateRoot};
use leptos::prelude::*;
use std::collections::HashMap;
use std::rc::Rc;
use wasm_bindgen::JsCast;

async fn fetch_nomenclature() -> Result<Vec<Nomenclature>, String> {
    use wasm_bindgen::JsCast;
    use web_sys::{Request, RequestInit, RequestMode, Response};

    let opts = RequestInit::new();
    opts.set_method("GET");
    opts.set_mode(RequestMode::Cors);

    let url = format!("{}/api/nomenclature", api_base());
    let request = Request::new_with_str_and_init(&url, &opts).map_err(|e| format!("{e:?}"))?;
    request
        .headers()
        .set("Accept", "application/json")
        .map_err(|e| format!("{e:?}"))?;

    let window = web_sys::window().ok_or_else(|| "no window".to_string())?;
    let resp_value = wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&request))
        .await
        .map_err(|e| format!("{e:?}"))?;
    let resp: Response = resp_value.dyn_into().map_err(|e| format!("{e:?}"))?;

    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }

    let text = wasm_bindgen_futures::JsFuture::from(resp.text().map_err(|e| format!("{e:?}"))?)
        .await
        .map_err(|e| format!("{e:?}"))?;
    let text: String = text.as_string().ok_or_else(|| "bad text".to_string())?;
    let data: Vec<Nomenclature> = serde_json::from_str(&text).map_err(|e| format!("{e}"))?;
    Ok(data)
}

#[derive(Clone)]
struct TreeNode {
    item: Nomenclature,
    children: Vec<TreeNode>,
    expanded: RwSignal<bool>,
}

/// Правильное построение дерева: сначала группируем детей, потом строим узлы
fn build_tree(items: Vec<Nomenclature>) -> Vec<TreeNode> {
    use contracts::domain::common::AggregateId;

    if items.is_empty() {
        return vec![];
    }

    // Создаем set всех существующих ID для проверки валидности parent_id
    let existing_ids: std::collections::HashSet<String> =
        items.iter().map(|item| item.base.id.as_string()).collect();

    // Подсчитываем папки
    let folders_count = items.iter().filter(|i| i.is_folder).count();

    // Группируем детей по parent_id
    let mut children_map: HashMap<Option<String>, Vec<Nomenclature>> = HashMap::new();
    for item in items {
        let parent_id = item.parent_id.clone();

        // Если parent_id указан, но родителя нет в списке - считаем элемент корневым
        let normalized_parent = if let Some(ref pid) = parent_id {
            // 00000000-0000-0000-0000-000000000000 - пустой GUID из 1С, эквивалент NULL
            if pid == "00000000-0000-0000-0000-000000000000" {
                None
            } else if existing_ids.contains(pid) {
                Some(pid.clone())
            } else {
                web_sys::console::warn_1(
                    &format!(
                        "Item {} has invalid parent_id: {}",
                        item.base.id.as_string(),
                        pid
                    )
                    .into(),
                );
                None
            }
        } else {
            None
        };

        children_map
            .entry(normalized_parent)
            .or_insert_with(Vec::new)
            .push(item);
    }

    // Рекурсивная функция для построения узла со всеми его детьми
    fn build_node(
        item: Nomenclature,
        children_map: &HashMap<Option<String>, Vec<Nomenclature>>,
    ) -> TreeNode {
        let id = item.base.id.as_string();
        let children = children_map
            .get(&Some(id.clone()))
            .map(|kids| {
                kids.iter()
                    .map(|kid| build_node(kid.clone(), children_map))
                    .collect()
            })
            .unwrap_or_else(Vec::new);

        TreeNode {
            item,
            children,
            expanded: RwSignal::new(false),
        }
    }

    // Сортировка узлов
    fn sort_nodes(nodes: &mut Vec<TreeNode>) {
        nodes.sort_by(|a, b| match (a.item.is_folder, b.item.is_folder) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a
                .item
                .base
                .description
                .to_lowercase()
                .cmp(&b.item.base.description.to_lowercase()),
        });
        for n in nodes.iter_mut() {
            if !n.children.is_empty() {
                sort_nodes(&mut n.children);
            }
        }
    }

    // Строим корневые узлы (без parent_id или с несуществующим parent_id)
    let mut roots = children_map
        .get(&None)
        .map(|root_items| {
            root_items
                .iter()
                .map(|item| build_node(item.clone(), &children_map))
                .collect()
        })
        .unwrap_or_else(Vec::new);

    sort_nodes(&mut roots);

    let folders_with_children = children_map.iter().filter(|(key, _)| key.is_some()).count();

    web_sys::console::log_1(
        &format!(
            "build_tree: {} total items, {} root nodes, {} groups, {} folders ({} with children)",
            existing_ids.len(),
            roots.len(),
            children_map.len(),
            folders_count,
            folders_with_children
        )
        .into(),
    );

    roots
}

/// Сортировка узлов дерева (всегда папки первыми, затем по полю)
fn sort_tree_nodes(nodes: &mut Vec<TreeNode>, sort_field: &str, ascending: bool) {
    nodes.sort_by(|a, b| {
        // Всегда папки первыми
        match (a.item.is_folder, b.item.is_folder) {
            (true, false) => return std::cmp::Ordering::Less,
            (false, true) => return std::cmp::Ordering::Greater,
            _ => {}
        }

        // Затем сортируем по выбранному полю
        let cmp = match sort_field {
            "code" => a
                .item
                .base
                .code
                .to_lowercase()
                .cmp(&b.item.base.code.to_lowercase()),
            "article" => a
                .item
                .article
                .to_lowercase()
                .cmp(&b.item.article.to_lowercase()),
            _ => a
                .item
                .base
                .description
                .to_lowercase()
                .cmp(&b.item.base.description.to_lowercase()),
        };

        if ascending {
            cmp
        } else {
            cmp.reverse()
        }
    });

    // Рекурсивно сортируем детей
    for node in nodes.iter_mut() {
        if !node.children.is_empty() {
            sort_tree_nodes(&mut node.children, sort_field, ascending);
        }
    }
}

/// Фильтрация дерева: возвращает узлы, соответствующие фильтру (рекурсивно)
fn filter_tree(nodes: Vec<TreeNode>, filter: &str, show_only_mp: bool) -> Vec<TreeNode> {
    let mut result = Vec::new();

    for node in nodes {
        // Проверка текстового фильтра
        let text_matches = if filter.trim().is_empty() || filter.trim().len() < 3 {
            true
        } else {
            let filter_lower = filter.to_lowercase();
            node.item
                .base
                .description
                .to_lowercase()
                .contains(&filter_lower)
                || node.item.base.code.to_lowercase().contains(&filter_lower)
                || node.item.article.to_lowercase().contains(&filter_lower)
        };

        // Проверка фильтра "Только из маркетплейсов"
        let mp_matches = if show_only_mp {
            // Для элементов (не папок) проверяем mp_ref_count > 0
            if !node.item.is_folder {
                node.item.mp_ref_count > 0
            } else {
                // Для папок всегда true, но проверим детей ниже
                true
            }
        } else {
            true
        };

        // Рекурсивно фильтруем детей
        let filtered_children = filter_tree(node.children.clone(), filter, show_only_mp);

        // Включаем узел если:
        // 1. Он сам подходит по всем фильтрам
        // 2. Или у него есть дети, которые подходят (для папок)
        let should_include = if node.item.is_folder {
            // Папка включается если у нее есть подходящие дети
            // (не проверяем text_matches для папки, только наличие детей)
            !filtered_children.is_empty()
        } else {
            // Элемент включается если он сам подходит по всем фильтрам
            text_matches && mp_matches
        };

        if should_include {
            let new_node = TreeNode {
                item: node.item.clone(),
                children: filtered_children,
                expanded: node.expanded,
            };
            // Авто-раскрываем узлы при фильтрации
            if filter.trim().len() >= 3 && !new_node.children.is_empty() {
                new_node.expanded.set(true);
            }
            result.push(new_node);
        }
    }

    result
}

/// Подсветка совпадений в тексте (case-insensitive)
fn highlight_matches(text: &str, filter: &str) -> AnyView {
    if filter.trim().is_empty() || filter.trim().len() < 3 {
        return view! { <span>{text}</span> }.into_any();
    }

    let filter_lower = filter.to_lowercase();
    let text_lower = text.to_lowercase();

    // Если нет совпадений, возвращаем текст как есть
    if !text_lower.contains(&filter_lower) {
        return view! { <span>{text}</span> }.into_any();
    }

    // Находим все совпадения
    let mut parts: Vec<AnyView> = Vec::new();
    let mut last_pos = 0;

    while let Some(pos) = text_lower[last_pos..].find(&filter_lower) {
        let actual_pos = last_pos + pos;

        // Добавляем текст до совпадения
        if actual_pos > last_pos {
            parts.push(view! { <span>{&text[last_pos..actual_pos]}</span> }.into_any());
        }

        // Добавляем подсвеченное совпадение
        let match_end = actual_pos + filter_lower.len();
        parts.push(view! {
            <span style="background-color: #ff9800; color: white; padding: 1px 2px; border-radius: 2px; font-weight: 500;">
                {&text[actual_pos..match_end]}
            </span>
        }.into_any());

        last_pos = match_end;
    }

    // Добавляем оставшийся текст
    if last_pos < text.len() {
        parts.push(view! { <span>{&text[last_pos..]}</span> }.into_any());
    }

    view! { <>{parts}</> }.into_any()
}

fn render_rows_with_lookup(
    node: TreeNode,
    level: usize,
    id_to_label: HashMap<String, String>,
    on_open: Rc<dyn Fn(String)>,
    filter: String,
) -> Vec<AnyView> {
    let mut rows: Vec<AnyView> = Vec::new();

    let has_children = !node.children.is_empty();
    let expanded = node.expanded;
    let label = node.item.base.description.clone();
    let code = node.item.base.code.clone();
    let id = node.item.base.id.as_string();
    let is_folder = node.item.is_folder;
    let article = node.item.article.clone();

    // Кнопка раскрытия/закрытия
    let toggle: AnyView = if is_folder && has_children {
        let chevron_icon = move || {
            if expanded.get() {
                icon("chevron-down")
            } else {
                icon("chevron-right")
            }
        };
        view! {
            <button
                class="tree-toggle"
                style="background: none; border: none; cursor: pointer; padding: 0; display: inline-flex; align-items: center; color: var(--color-text-secondary);"
                on:click=move |_| expanded.update(|v| *v = !*v)
            >
                {chevron_icon}
            </button>
        }.into_any()
    } else {
        view! { <span style="display:inline-block; width: 16px;">{""}</span> }.into_any()
    };

    // Иконка узла
    let node_icon_view = if is_folder {
        if has_children && expanded.get() {
            view! { <span style="color: #f4b942;">{icon("folder-open")}</span> }.into_any()
        } else {
            view! { <span style="color: #f4b942;">{icon("folder-closed")}</span> }.into_any()
        }
    } else {
        view! { <span style="color: var(--color-text-muted);">{icon("item")}</span> }.into_any()
    };

    let open = {
        let on_open = on_open.clone();
        let id_clone = id.clone();
        move |_| (on_open)(id_clone.clone())
    };

    // Подсветка текста в зависимости от фильтра
    let label_view = highlight_matches(&label, &filter);
    let article_view = if is_folder {
        view! { <span>{""}</span> }.into_any()
    } else {
        highlight_matches(&article, &filter)
    };
    let code_view = highlight_matches(&code, &filter);

    let row = view! {
        <tr>
            <td class="text-center p-0-8 whitespace-nowrap" style="width: 40px;">
                <input type="checkbox" style="margin: 0; cursor: pointer;"/>
            </td>
            <td class="text-center p-0-8 whitespace-nowrap" style="width: 40px;">
                <div class="icon-cell-container">
                    {node_icon_view}
                </div>
            </td>
            <td class="cell-truncate p-0-8">
                <div style={format!(
                    "display: flex; align-items: center; gap: 6px; padding-left: {}px;",
                    level * 16
                )}>
                    {toggle}
                    <span class="tree-label" on:click=open>
                        {label_view}
                    </span>
                </div>
            </td>
            <td class="cell-truncate p-0-8">{article_view}</td>
            <td class="cell-truncate p-0-8">{code_view}</td>
        </tr>
    }
    .into_any();

    rows.push(row);

    if expanded.get() {
        for child in node.children.clone().into_iter() {
            let mut child_rows = render_rows_with_lookup(
                child,
                level + 1,
                id_to_label.clone(),
                on_open.clone(),
                filter.clone(),
            );
            rows.append(&mut child_rows);
        }
    }

    rows
}

#[component]
pub fn NomenclatureTree() -> impl IntoView {
    let modal_stack =
        use_context::<ModalStackService>().expect("ModalStackService not found in context");
    let (all_roots, set_all_roots) = signal::<Vec<TreeNode>>(vec![]);
    let (error, set_error) = signal::<Option<String>>(None);
    let (show_modal, set_show_modal) = signal(false);
    let (editing_id, set_editing_id) = signal::<Option<String>>(None);
    let (id_to_label, set_id_to_label) = signal::<HashMap<String, String>>(HashMap::new());
    let (filter_text, set_filter_text) = signal(String::new());
    let (filter_input, set_filter_input) = signal(String::new()); // Для debounce
    let (show_only_mp, set_show_only_mp) = signal(false); // Фильтр "Только из маркетплейсов"
    let (is_loading, set_is_loading) = signal(false);
    let (sort_field, set_sort_field) = signal::<String>("description".to_string());
    let (sort_ascending, set_sort_ascending) = signal(true);

    // Простой debounce механизм: обновляем filter_text только после паузы ввода
    let debounce_timeout = leptos::prelude::StoredValue::new(None::<i32>);
    let handle_input_change = move |value: String| {
        set_filter_input.set(value.clone());

        // Отменяем предыдущий таймер если есть
        if let Some(timeout_id) = debounce_timeout.get_value() {
            web_sys::window().and_then(|w| Some(w.clear_timeout_with_handle(timeout_id)));
        }

        // Создаем новый таймер
        let window = web_sys::window().expect("no window");
        let closure = wasm_bindgen::closure::Closure::wrap(Box::new(move || {
            set_filter_text.set(value.clone());
        }) as Box<dyn Fn()>);

        let timeout_id = window
            .set_timeout_with_callback_and_timeout_and_arguments_0(
                closure.as_ref().unchecked_ref(),
                300, // 300ms задержка
            )
            .expect("setTimeout failed");

        closure.forget();
        debounce_timeout.set_value(Some(timeout_id));
    };

    let load = move || {
        set_is_loading.set(true);
        set_error.set(None);
        wasm_bindgen_futures::spawn_local(async move {
            match fetch_nomenclature().await {
                Ok(list) => {
                    web_sys::console::log_1(
                        &format!("Loaded {} nomenclature items", list.len()).into(),
                    );
                    let map: HashMap<String, String> = list
                        .iter()
                        .map(|c| (c.base.id.as_string(), c.base.description.clone()))
                        .collect();
                    set_id_to_label.set(map);
                    let tree = build_tree(list);
                    web_sys::console::log_1(
                        &format!("Built tree with {} roots", tree.len()).into(),
                    );
                    set_all_roots.set(tree);
                    set_error.set(None);
                    set_is_loading.set(false);
                }
                Err(e) => {
                    web_sys::console::error_1(&format!("Error loading nomenclature: {}", e).into());
                    set_error.set(Some(format!("Ошибка загрузки: {}", e)));
                    set_is_loading.set(false);
                }
            }
        });
    };

    // Вычисляемое значение для отфильтрованного и отсортированного дерева
    let filtered_roots = move || {
        let roots = all_roots.get();
        let filter = filter_text.get();
        let only_mp = show_only_mp.get();
        let mut filtered = filter_tree(roots, &filter, only_mp);
        let field = sort_field.get();
        let ascending = sort_ascending.get();
        sort_tree_nodes(&mut filtered, &field, ascending);
        filtered
    };

    load();

    let open_details_modal = move |id: Option<String>| {
        let id_val = id.clone();
        modal_stack.push_with_frame(
            Some("max-width: min(1400px, 95vw); width: min(1400px, 95vw);".to_string()),
            Some("nomenclature-modal".to_string()),
            move |handle| {
                view! {
                    <NomenclatureDetails
                        id=id_val.clone()
                        on_saved=Callback::new({
                            let handle = handle.clone();
                            move |_| {
                                handle.close();
                                load();
                            }
                        })
                        on_cancel=Callback::new({
                            let handle = handle.clone();
                            move |_| handle.close()
                        })
                    />
                }
                .into_any()
            },
        );
    };

    let list_name = Nomenclature::list_name();

    // Проверка активности фильтра
    let is_filter_active = move || {
        let text = filter_text.get();
        !text.trim().is_empty() && text.trim().len() >= 3
    };

    // Обработчики сортировки
    let toggle_sort = move |field: &'static str| {
        move |_| {
            if sort_field.get() == field {
                set_sort_ascending.update(|v| *v = !*v);
            } else {
                set_sort_field.set(field.to_string());
                set_sort_ascending.set(true);
            }
        }
    };

    // Функция для получения индикатора сортировки
    let sort_indicator = move |field: &'static str| -> &'static str {
        if sort_field.get() == field {
            if sort_ascending.get() {
                " ▲"
            } else {
                " ▼"
            }
        } else {
            " ⇅"
        }
    };

    view! {
        <div class="content">
            <div class="page__header" style="margin-bottom: 8px; flex-shrink: 0;">
                <div class="page__header-left">
                    <h2 class="page__title" style="margin: 0;">{list_name}</h2>
                </div>
                <div class="page__header-right" style="display: flex; align-items: center; gap: 8px;">
                    <div style="position: relative; display: inline-flex; align-items: center;">
                        <input
                            type="text"
                            placeholder="Поиск (мин. 3 символа)..."
                            style=move || format!(
                                "width: 250px; padding: 6px 32px 6px 10px; border: 1px solid var(--form-input-border); border-radius: 4px; font-size: 15px; background: {};",
                                if is_filter_active() { "var(--warning-box-bg)" } else { "var(--form-input-bg)" }
                            )
                            prop:value=move || filter_input.get()
                            on:input=move |ev| {
                                let val = event_target_value(&ev);
                                handle_input_change(val);
                            }
                        />
                        {move || if !filter_input.get().is_empty() {
                            view! {
                                <button
                                    style="position: absolute; right: 6px; background: none; border: none; cursor: pointer; padding: 4px; display: inline-flex; align-items: center; color: var(--color-text-secondary); line-height: 1;"
                                    on:click=move |_| {
                                        set_filter_input.set(String::new());
                                        set_filter_text.set(String::new());
                                    }
                                    title="Очистить"
                                >
                                    {icon("x")}
                                </button>
                            }.into_any()
                        } else {
                            view! { <></> }.into_any()
                        }}
                    </div>
                    <label style="display: inline-flex; align-items: center; gap: 6px; cursor: pointer; user-select: none; font-size: 15px;">
                        <input
                            type="checkbox"
                            prop:checked=move || show_only_mp.get()
                            on:change=move |ev| {
                                set_show_only_mp.set(event_target_checked(&ev));
                            }
                            style="cursor: pointer;"
                        />
                        <span>{"Только из маркетплейсов"}</span>
                    </label>
                    <button class="button button--primary" on:click=move |_| { set_editing_id.set(None); set_show_modal.set(true); }>
                        {icon("plus")}
                        {"Новый"}
                    </button>
                    <button class="button button--secondary" on:click=move |_| load()>
                        {icon("refresh")}
                        {"Обновить"}
                    </button>
                </div>
            </div>
            {move || error.get().map(|e| view! { <div class="error" style="background: #fee; color: #c33; padding: 8px; border-radius: 4px; margin-bottom: 8px; font-size: 15px; flex-shrink: 0;">{e}</div> })}

            {move || if is_loading.get() {
                view! { <div style="text-align: center; padding: 20px; color: var(--color-text-secondary);">{"⏳ Загрузка..."}</div> }.into_any()
            } else {
                view! {
                    <>
                        <div class="table-container">
                            <table class="tree-table">
                                <thead>
                                    <tr class="text-left">
                                        <th class="text-center whitespace-nowrap p-0-8" style="width: 40px; border-bottom: 2px solid var(--color-border);">
                                            <input type="checkbox" style="margin: 0; cursor: pointer;"/>
                                        </th>
                                        <th class="text-center whitespace-nowrap p-0-8" style="width: 40px; border-bottom: 2px solid var(--color-border);">{""}</th>
                                        <th
                                            class="th-w-40p whitespace-nowrap cursor-pointer user-select-none p-0-8"
                                            style="border-bottom: 2px solid var(--color-border);"
                                            title="Сортировать"
                                            on:click=toggle_sort("description")
                                        >
                                            {move || format!("Наименование{}", sort_indicator("description"))}
                                        </th>
                                        <th
                                            class="th-w-20p whitespace-nowrap cursor-pointer user-select-none p-0-8"
                                            style="border-bottom: 2px solid var(--color-border);"
                                            title="Сортировать"
                                            on:click=toggle_sort("article")
                                        >
                                            {move || format!("Артикул{}", sort_indicator("article"))}
                                        </th>
                                        <th
                                            class="th-w-40p whitespace-nowrap cursor-pointer user-select-none p-0-8"
                                            style="border-bottom: 2px solid var(--color-border);"
                                            title="Сортировать"
                                            on:click=toggle_sort("code")
                                        >
                                            {move || format!("Код{}", sort_indicator("code"))}
                                        </th>
                                    </tr>
                                </thead>
                                <tbody>
                                    {move || {
                                        let lookup = id_to_label.get();
                                        let roots = filtered_roots();
                                        let current_filter = filter_text.get();
                                        if roots.is_empty() {
                                            let all_count = all_roots.get().len();
                                            let msg = if all_count == 0 {
                                                "Нет данных. Нажмите 'Обновить' или загрузите данные через импорт из 1С."
                                            } else {
                                                "По фильтру ничего не найдено"
                                            };
                                            view! { <tr><td colspan="5" class="text-center" style="color: var(--color-text-muted); padding: 20px;">{msg}</td></tr> }.into_any()
                                        } else {
                                            let all_rows = roots
                                                .into_iter()
                                                .flat_map(move |n| render_rows_with_lookup(n, 0, lookup.clone(), Rc::new(move |id: String| { set_editing_id.set(Some(id)); set_show_modal.set(true); }), current_filter.clone()))
                                                .collect::<Vec<_>>();
                                            all_rows.into_view().into_any()
                                        }
                                    }}
                                </tbody>
                            </table>
                        </div>
                    </>
                }.into_any()
            }}

            {move || if show_modal.get() {
                open_details_modal(editing_id.get());
                set_show_modal.set(false);
                set_editing_id.set(None);
                view! { <></> }.into_any()
            } else { view! { <></> }.into_any() }}
        </div>
    }
}
