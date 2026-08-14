use super::DomNode;
use std::collections::HashMap;
use wasm_bindgen::JsCast;
use web_sys::{Element, Node};

/// Tab key of the inspector itself — its own tab is skipped so the tree shows
/// the page under inspection, not the inspector. Without this the snapshot taken
/// from the header (before the tab opens) and the one taken by "Обновить снимок"
/// (with the tab open) would disagree.
const SELF_TAB_KEY: &str = "dom_inspector";

pub fn build_dom_tree() -> Option<DomNode> {
    let window = web_sys::window()?;
    let document = window.document()?;
    let body = document.body()?;

    Some(build_node_tree(&body.into(), 0))
}

/// True for the `.app-tabs__item` wrapper belonging to the inspector tab.
fn is_self_tab(element: &Element) -> bool {
    element.get_attribute("data-tab-key").as_deref() == Some(SELF_TAB_KEY)
}

fn collect_text_from_node(node: &Node) -> String {
    let mut text_parts = Vec::new();

    // Если это текстовый узел, добавляем его содержимое
    if node.node_type() == Node::TEXT_NODE {
        if let Some(text) = node.text_content() {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                text_parts.push(trimmed.to_string());
            }
        }
    }

    // Рекурсивно обходим все дочерние узлы
    let children = node.child_nodes();
    for i in 0..children.length() {
        if let Some(child) = children.get(i) {
            let child_text = collect_text_from_node(&child);
            if !child_text.is_empty() {
                text_parts.push(child_text);
            }
        }
    }

    text_parts.join(" ")
}

fn get_text_content(element: &Element) -> Option<String> {
    let text = collect_text_from_node(element.as_ref());
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn build_node_tree(element: &Element, depth: usize) -> DomNode {
    let tag_name = element.tag_name().to_lowercase();

    // Получаем id элемента
    let id = element.get_attribute("id");

    // Получаем классы
    let classes = element
        .class_list()
        .value()
        .split_whitespace()
        .map(String::from)
        .collect::<Vec<_>>();

    // Собираем data-* атрибуты
    let mut data_attributes = HashMap::new();

    // Проверяем основные data-атрибуты вручную
    let data_attrs_to_check = vec![
        "data-tab-key",
        "data-component",
        "data-id",
        "data-state",
        "data-page-category",
    ];
    for attr_name in data_attrs_to_check {
        if let Some(value) = element.get_attribute(attr_name) {
            data_attributes.insert(attr_name.to_string(), value);
        }
    }

    // Если это button, собираем текст
    let button_text = if tag_name == "button" {
        get_text_content(element)
    } else {
        None
    };

    // Фильтруем только нужные теги
    let allowed_tags = [
        "div", "table", "thead", "tbody", "tfoot", "tr", "th", "td", "button",
    ];

    let mut children = Vec::new();

    let child_nodes = element.child_nodes();
    for i in 0..child_nodes.length() {
        if let Some(child_node) = child_nodes.get(i) {
            if let Some(child_element) = child_node.dyn_ref::<Element>() {
                let child_tag = child_element.tag_name().to_lowercase();

                if allowed_tags.contains(&child_tag.as_str()) && !is_self_tab(child_element) {
                    children.push(build_node_tree(child_element, depth + 1));
                }
            }
        }
    }

    DomNode {
        tag_name,
        id,
        classes,
        data_attributes,
        button_text,
        children,
        depth,
    }
}
