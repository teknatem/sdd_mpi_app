use super::DomNode;
use crate::shared::icons::icon;
use leptos::prelude::*;

#[component]
pub fn TreeView(node: DomNode) -> impl IntoView {
    let has_children = !node.children.is_empty();
    let is_table = node.tag_name == "table";

    // Проверяем специальные классы для сворачивания
    let is_panel_left = node.classes.iter().any(|c| c == "panel-left");
    let is_right_panel = node.classes.iter().any(|c| c == "right-panel");
    let is_app_header = node.classes.iter().any(|c| c == "app-header");
    let is_app_sidebar = node.classes.iter().any(|c| c == "app-sidebar");
    let is_page = node.classes.iter().any(|c| c.starts_with("page"));

    // Автоматически сворачивать определенные элементы
    let initially_collapsed = is_table
        || is_panel_left
        || is_right_panel
        || is_app_header
        || is_app_sidebar
        || node.depth >= 8;
    let (is_collapsed, set_is_collapsed) = signal(initially_collapsed);
    let children = node.children.clone();

    let toggle = move |_| {
        if has_children {
            set_is_collapsed.update(|val| *val = !*val);
        }
    };

    view! {
        <div class="dom-tree-node">
            <div
                class="dom-tree-node__header"
                class:dom-tree-node__header--clickable=has_children
                on:click=toggle
            >
                // Иконка сворачивания
                {move || if has_children {
                    if is_collapsed.get() {
                        view! { <span class="dom-tree-node__icon">{icon("chevron-right")}</span> }.into_any()
                    } else {
                        view! { <span class="dom-tree-node__icon">{icon("chevron-down")}</span> }.into_any()
                    }
                } else {
                    view! { <span class="dom-tree-node__icon dom-tree-node__icon--empty"></span> }.into_any()
                }}

                // Тег
                <span
                    class="dom-tree-node__tag"
                    class:dom-tree-node__tag--table=is_table
                    class:dom-tree-node__tag--panel-left=is_panel_left
                    class:dom-tree-node__tag--right-panel=is_right_panel
                    class:dom-tree-node__tag--page=is_page
                >
                    {format!("<{}>", node.tag_name)}
                </span>

                // Уровень вложенности
                <span class="dom-tree-node__depth">
                    {format!("[{}]", node.depth)}
                </span>

                // ID элемента
                {node.id.as_ref().map(|id_value| {
                    view! {
                        <span class="dom-tree-node__id">
                            {format!("#{}", id_value)}
                        </span>
                    }
                })}

                // Классы
                {(!node.classes.is_empty()).then(|| {
                    view! {
                        <span class="dom-tree-node__classes">
                            {node.classes.iter().map(|cls| {
                                let class_text = format!(".{}", cls);
                                let is_special = cls == "panel-left"
                                    || cls == "right-panel"
                                    || cls == "app-header"
                                    || cls == "app-sidebar"
                                    || cls == "app-main"
                                    || cls == "app-panel"
                                    || cls.starts_with("page");
                                view! {
                                    <span
                                        class="dom-tree-node__class"
                                        class:dom-tree-node__class--special=is_special
                                    >
                                        {class_text}
                                    </span>
                                }
                            }).collect_view()}
                        </span>
                    }
                })}

                // Data-атрибуты
                {(!node.data_attributes.is_empty()).then(|| {
                    let has_hidden_class = node.classes.iter().any(|c| c == "app-tabs__item--hidden");

                    view! {
                        <span class="dom-tree-node__data-attrs">
                            {node.data_attributes.iter().map(|(key, value)| {
                                let is_tab_key_hidden = key == "data-tab-key" && has_hidden_class;
                                view! {
                                    <span
                                        class="dom-tree-node__data-attr"
                                        class:dom-tree-node__data-attr--hidden=is_tab_key_hidden
                                    >
                                        {format!("{}=\"{}\"", key, value)}
                                    </span>
                                }
                            }).collect_view()}
                        </span>
                    }
                })}

                // Текст кнопки
                {node.button_text.as_ref().map(|text| {
                    view! {
                        <span class="dom-tree-node__button-text">
                            {format!("\"{}\"", text)}
                        </span>
                    }
                })}
            </div>

            // Дети
            <Show when=move || has_children && !is_collapsed.get()>
                <div class="dom-tree-node__children">
                    {children.iter().cloned().map(|child| {
                        view! { <TreeView node=child /> }.into_any()
                    }).collect_view()}
                </div>
            </Show>
        </div>
    }
}
